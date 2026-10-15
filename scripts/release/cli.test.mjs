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
