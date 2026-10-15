import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const cli = fileURLToPath(new URL('./cli.mjs', import.meta.url));
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const run = (args, env) =>
  execFileSync(process.execPath, [cli, ...args], { env: { ...process.env, ...env }, encoding: 'utf8', stdio: 'pipe' });

function scratch() {
  const dir = mkdtempSync(join(tmpdir(), 'perland-sign-'));
  const exe = join(dir, 'perland-launcher.exe');
  writeFileSync(exe, 'unsigned launcher');
  mkdirSync(join(dir, 'Plugins', 'x86-unicode'), { recursive: true });
  const plugin = join(dir, 'Plugins', 'x86-unicode', 'System.dll');
  writeFileSync(plugin, 'plugin');
  return { exe, plugin, signDir: join(dir, 'sign') };
}

test('collect copies our binaries out by hash and leaves files and plugins untouched', () => {
  const { exe, plugin, signDir } = scratch();
  const out = run(['sign-collect', exe], { SIGN_DIR: signDir });
  run(['sign-collect', plugin], { SIGN_DIR: signDir });
  assert.deepEqual(readdirSync(join(signDir, 'unsigned')), [`${sha('unsigned launcher')}.exe`]);
  assert.equal(readFileSync(exe, 'utf8'), 'unsigned launcher');
  assert.match(out, /sign-collect binary [0-9a-f]{64} /);
});

test('apply swaps in the signed copy with the same hash', () => {
  const { exe, plugin, signDir } = scratch();
  mkdirSync(join(signDir, 'signed'), { recursive: true });
  writeFileSync(join(signDir, 'signed', `${sha('unsigned launcher')}.exe`), 'signed launcher');
  run(['sign-apply', exe], { SIGN_DIR: signDir });
  run(['sign-apply', plugin], { SIGN_DIR: signDir });
  assert.equal(readFileSync(exe, 'utf8'), 'signed launcher');
  assert.equal(readFileSync(plugin, 'utf8'), 'plugin');
});

test('apply fails on a binary round 1 never saw, and both commands need SIGN_DIR', () => {
  const { exe, signDir } = scratch();
  assert.throws(() => run(['sign-apply', exe], { SIGN_DIR: signDir }), /not reproducible/);
  assert.throws(() => run(['sign-collect', exe], { SIGN_DIR: '' }), /SIGN_DIR/);
});

test('every call is in the ledger, failures included, and sign-check fails the pass on them', () => {
  const { exe, plugin, signDir } = scratch();
  run(['sign-collect', exe], { SIGN_DIR: signDir });
  run(['sign-collect', plugin], { SIGN_DIR: signDir });
  assert.throws(() => run(['sign-apply', exe], { SIGN_DIR: signDir }), /not reproducible/);
  const ledger = readFileSync(join(signDir, 'ledger.log'), 'utf8').trim().split('\n');
  assert.equal(ledger.length, 3);
  assert.match(ledger[0], new RegExp(`^sign-collect binary ${sha('unsigned launcher')} collected `));
  assert.match(ledger[1], /^sign-collect plugin [0-9a-f]{64} skipped /);
  assert.match(ledger[2], /^sign-apply binary [0-9a-f]{64} error /);
  // One binary is not enough for pass 1, and pass 2 had an error.
  assert.throws(() => run(['sign-check', 'collect'], { SIGN_DIR: signDir }), /collected 1/);
  assert.throws(() => run(['sign-check', 'apply'], { SIGN_DIR: signDir }), /apply pass failed/);
});

test('sign-config writes the Tauri signCommand config with an absolute, forward-slash script path', () => {
  const { signDir } = scratch();
  mkdirSync(signDir, { recursive: true });
  const out = join(signDir, 'collect.json');
  run(['sign-config', 'collect', out], {});
  const command = JSON.parse(readFileSync(out, 'utf8')).bundle.windows.signCommand;
  assert.equal(command.cmd, 'node');
  assert.equal(command.args[0], cli.replaceAll('\\', '/'));
  assert.deepEqual(command.args.slice(1), ['sign-collect', '%1']);
  assert.throws(() => run(['sign-config', 'publish', out], {}), /collect or apply/);
});
