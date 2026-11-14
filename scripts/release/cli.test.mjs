import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { fileURLToPath } from 'node:url';

const cli = fileURLToPath(new URL('./cli.mjs', import.meta.url));
const work = mkdtempSync(join(tmpdir(), 'release-cli-'));
after(() => rmSync(work, { recursive: true, force: true }));

/** A minimal 64-bit PE image, signed when `signed` (a non-empty certificate table). */
function pe(signed) {
  const buf = Buffer.alloc(0x200);
  buf.write('MZ', 0, 'latin1');
  buf.writeUInt32LE(0x80, 0x3c);
  buf.write('PE\0\0', 0x80, 'latin1');
  buf.writeUInt16LE(0x20b, 0x80 + 24);
  if (signed) buf.writeUInt32LE(0x2a8, 0x80 + 24 + 112 + 4 * 8 + 4);
  return buf;
}

function run(args, env) {
  return spawnSync(process.execPath, [cli, ...args], { encoding: 'utf8', env: { ...process.env, ...env } });
}

test('sign skips a plugin DLL that is already signed, without calling ssign', () => {
  const dir = join(work, 'skip');
  const plugin = join(dir, 'nsis', 'Plugins', 'x86-unicode', 'additional', 'nsis_tauri_utils.dll');
  mkdirSync(join(plugin, '..'), { recursive: true });
  writeFileSync(plugin, pe(true));
  const r = run(['sign', plugin], { SIGN_DIR: join(dir, 'sign'), SSIGN: join(dir, 'no-such-ssign.exe') });
  assert.equal(r.status, 0, r.stderr);
  assert.match(readFileSync(join(dir, 'sign', 'ledger.log'), 'utf8'), /^plugin already-signed .*nsis_tauri_utils\.dll\n$/);
});

test('sign records a failure in the ledger and exits non-zero', () => {
  const dir = join(work, 'fail');
  const exe = join(dir, 'uninstall.exe');
  mkdirSync(dir, { recursive: true });
  writeFileSync(exe, pe(false));
  const r = run(['sign', exe], { SIGN_DIR: join(dir, 'sign'), SSIGN: join(dir, 'no-such-ssign.exe') });
  assert.notEqual(r.status, 0);
  assert.match(readFileSync(join(dir, 'sign', 'ledger.log'), 'utf8'), /^binary error .*uninstall\.exe\n$/);
});

test('sign-check fails on that ledger', () => {
  const r = run(['sign-check'], { SIGN_DIR: join(work, 'fail', 'sign') });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /Signing failed for .*uninstall\.exe/);
});

test('sign-config points Tauri at this CLI, with %1 for the file', () => {
  const out = join(work, 'sign.json');
  const r = run(['sign-config', out], {});
  assert.equal(r.status, 0, r.stderr);
  const command = JSON.parse(readFileSync(out, 'utf8')).bundle.windows.signCommand;
  assert.equal(command.cmd, 'node');
  assert.deepEqual(command.args.slice(1), ['sign', '%1']);
  assert.match(command.args[0], /scripts\/release\/cli\.mjs$/);
});
