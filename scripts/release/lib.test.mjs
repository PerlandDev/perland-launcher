import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  buildManifest,
  channelName,
  checkSignatureKeys,
  ciGate,
  clientIdGate,
  findBundle,
  keyIdOf,
  nextVersion,
  PLATFORMS,
  releaseName,
  setWorkspaceVersion,
} from './lib.mjs';

test('the first beta of a base version is beta.1', () => {
  assert.equal(nextVersion({ base: '0.2.0', channel: 'beta', tags: ['v0.1.0-beta.3'] }), '0.2.0-beta.1');
});

test('betas count up from the highest, ignoring odd tags', () => {
  const tags = ['v0.2.0-beta.1', 'v0.2.0-beta.10', 'v0.2.0-beta.2', 'v0.2.0-beta.x', 'channel-beta'];
  assert.equal(nextVersion({ base: '0.2.0', channel: 'beta', tags }), '0.2.0-beta.11');
});

test('stable is the base, once', () => {
  assert.equal(nextVersion({ base: '0.2.0', channel: 'stable', tags: ['v0.2.0-beta.4'] }), '0.2.0');
  assert.throws(() => nextVersion({ base: '0.2.0', channel: 'stable', tags: ['v0.2.0'] }), /already released/);
});

test('no beta after its stable, no bad input', () => {
  assert.throws(() => nextVersion({ base: '0.2.0', channel: 'beta', tags: ['v0.2.0'] }), /bump the version/);
  assert.throws(() => nextVersion({ base: '0.2', channel: 'beta', tags: [] }), /x\.y\.z/);
  assert.throws(() => nextVersion({ base: '0.2.0', channel: 'nightly', tags: [] }), /channel/);
});

test('the workspace version is replaced and nothing else', () => {
  const toml = '[workspace.package]\nversion = "0.2.0"\nedition = "2021"\n\n[workspace.dependencies]\nfoo = { version = "1" }\n';
  assert.equal(setWorkspaceVersion(toml, '0.2.0-beta.3'), toml.replace('version = "0.2.0"', 'version = "0.2.0-beta.3"'));
  assert.throws(() => setWorkspaceVersion(toml, ''), /Not a version/);
  assert.throws(() => setWorkspaceVersion(toml, 'latest'), /Not a version/);
  assert.throws(() => setWorkspaceVersion(toml, undefined), /Not a version/);
  assert.throws(() => setWorkspaceVersion('[package]\nversion = "1.0.0"\n', '1.0.1'), /workspace\.package/);
});

test('installers get safe release and channel names', () => {
  assert.equal(releaseName('windows-x86_64-nsis', '0.2.0-beta.1'), 'PerlandLauncher_0.2.0-beta.1_x64-setup.exe');
  assert.equal(releaseName('linux-x86_64-appimage', '0.2.0-beta.1'), 'PerlandLauncher_0.2.0-beta.1_amd64.AppImage');
  assert.equal(releaseName('linux-x86_64-deb', '0.2.0-beta.1'), 'PerlandLauncher_0.2.0-beta.1_amd64.deb');
  assert.equal(channelName('windows-x86_64-nsis', 'beta'), 'PerlandLauncher-beta-setup.exe');
  assert.equal(channelName('linux-x86_64-appimage', 'stable'), 'PerlandLauncher.AppImage');
  assert.equal(channelName('linux-x86_64-deb', 'beta'), 'PerlandLauncher-beta.deb');
});

test('bundles are found by platform, never their signatures', () => {
  const paths = [
    'nsis/Perland Launcher_0.2.0-beta.1_x64-setup.exe',
    'nsis/Perland Launcher_0.2.0-beta.1_x64-setup.exe.sig',
    'appimage/Perland Launcher_0.2.0-beta.1_amd64.AppImage',
    'deb/Perland Launcher_0.2.0-beta.1_amd64.deb',
  ];
  assert.equal(findBundle(paths, 'windows-x86_64-nsis'), paths[0]);
  assert.equal(findBundle(paths, 'linux-x86_64-appimage'), paths[2]);
  assert.equal(findBundle(paths, 'linux-x86_64-deb'), paths[3]);
  assert.equal(findBundle([], 'linux-x86_64-deb'), undefined);
});

test('the manifest lists every platform at the versioned release', () => {
  const signatures = Object.fromEntries(PLATFORMS.map((p) => [p, `sig-${p}`]));
  const manifest = buildManifest({
    version: '0.2.0-beta.1',
    notes: 'Closed beta',
    pubDate: '2026-10-15T10:00:00Z',
    signatures,
    baseUrl: 'https://github.com/PerlandDev/perland-launcher/releases/download/v0.2.0-beta.1',
  });
  assert.equal(manifest.version, '0.2.0-beta.1');
  assert.deepEqual(manifest.platforms['linux-x86_64-deb'], {
    signature: 'sig-linux-x86_64-deb',
    url: 'https://github.com/PerlandDev/perland-launcher/releases/download/v0.2.0-beta.1/PerlandLauncher_0.2.0-beta.1_amd64.deb',
  });
  assert.equal(Object.keys(manifest.platforms).length, 3);
});

test('a missing signature fails the manifest', () => {
  assert.throws(
    () => buildManifest({ version: '0.2.0', notes: '', pubDate: '2026-10-15T10:00:00Z', signatures: { 'windows-x86_64-nsis': 's' }, baseUrl: 'https://x' }),
    /linux-x86_64-appimage/,
  );
});

// Minisign-shaped blobs as Tauri stores them: base64 of a text file whose second line is base64 of
// [2-byte algorithm][8-byte key id][key or signature bytes]. Tiny and fake: no real keys.
const id = (n) => Array.from({ length: 8 }, (_, i) => n + i);
const b64 = (bytes) => Buffer.from(bytes).toString('base64');
const fakeKey = (keyId) =>
  b64(`untrusted comment: minisign public key\n${b64([0x45, 0x64, ...keyId, ...new Array(32).fill(7)])}\n`);
const fakeSig = (keyId) =>
  b64(
    `untrusted comment: signature from tauri secret key\n${b64([0x45, 0x44, ...keyId, ...new Array(64).fill(9)])}\n` +
      `trusted comment: timestamp:1\n${b64('global')}\n`,
  );

test('the key id is read from a public key and from a signature', () => {
  assert.equal(keyIdOf(fakeKey(id(1))), '0102030405060708');
  assert.equal(keyIdOf(fakeSig(id(1))), '0102030405060708');
  assert.notEqual(keyIdOf(fakeSig(id(2))), keyIdOf(fakeKey(id(1))));
});

test('anything that is not a minisign blob is refused', () => {
  for (const junk of ['', 'not base64 at all!', b64('one line only'), b64('comment\nAAAA\n'), b64(`c\n${b64('Xy12345678')}\n`)]) {
    assert.throws(() => keyIdOf(junk), /minisign/, JSON.stringify(junk));
  }
});

test('every signature must come from the committed public key', () => {
  const pubkey = fakeKey(id(1));
  const good = Object.fromEntries(PLATFORMS.map((p) => [p, fakeSig(id(1))]));
  assert.doesNotThrow(() => checkSignatureKeys(pubkey, good));
  assert.throws(() => checkSignatureKeys(pubkey, { ...good, 'linux-x86_64-deb': fakeSig(id(2)) }), /linux-x86_64-deb.*pubkey/s);
  assert.throws(() => checkSignatureKeys('', good), /pubkey/);
});

const run = (status, conclusion = null) => ({ status, conclusion });

test('a green CI run lets the release through', () => {
  assert.equal(ciGate([run('completed', 'success')], { publish: true }).level, 'ok');
  // A cancelled duplicate or a second run still going does not undo a green one.
  assert.equal(ciGate([run('completed', 'cancelled'), run('in_progress'), run('completed', 'success')], { publish: true }).level, 'ok');
});

test('a failed, running or missing CI run stops a publish', () => {
  const failed = ciGate([run('completed', 'success'), run('completed', 'failure')], { publish: true });
  assert.equal(failed.level, 'error');
  assert.match(failed.message, /failed/);
  assert.match(ciGate([run('completed', 'timed_out')], { publish: true }).message, /failed/);
  assert.match(ciGate([run('queued')], { publish: true }).message, /still running/);
  assert.match(ciGate([run('completed', 'cancelled')], { publish: true }).message, /has not passed/);
  assert.match(ciGate([], { publish: true }).message, /has not run/);
});

test('a dry run only warns about CI', () => {
  assert.equal(ciGate([run('completed', 'failure')], { publish: false }).level, 'warning');
  assert.equal(ciGate([], { publish: false }).level, 'warning');
});

test('a publish without the Microsoft client ID stops, since that build could not start the game', () => {
  for (const value of [undefined, '', '   ']) {
    const gate = clientIdGate(value, { publish: true });
    assert.equal(gate.level, 'error');
    assert.match(gate.message, /PERLAND_MS_CLIENT_ID/);
  }
});

test('a dry run only warns about a missing Microsoft client ID', () => {
  const gate = clientIdGate('', { publish: false });
  assert.equal(gate.level, 'warning');
  assert.match(gate.message, /PERLAND_MS_CLIENT_ID/);
});

test('a set Microsoft client ID lets the release through', () => {
  for (const publish of [true, false]) {
    assert.equal(clientIdGate('00000000-0000-4000-8000-0000000000aa', { publish }).level, 'ok');
  }
});
