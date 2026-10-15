import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  buildManifest,
  channelName,
  checkSignLedger,
  checkSignatureKeys,
  findBundle,
  keyIdOf,
  nextVersion,
  planApply,
  PLATFORMS,
  releaseName,
  releaseNotes,
  setWorkspaceVersion,
  signRole,
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
    notes: 'Beta build.',
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

test('release notes depend on the channel and on code signing', () => {
  const warning = " The installers aren't code-signed yet, so Windows may warn before it runs.";
  assert.equal(releaseNotes({ channel: 'beta', signed: true }), 'Beta build. Download it from https://perland.online/launcher.');
  assert.equal(releaseNotes({ channel: 'stable', signed: true }), 'Download it from https://perland.online/launcher.');
  assert.equal(releaseNotes({ channel: 'beta', signed: false }), `Beta build. Download it from https://perland.online/launcher.${warning}`);
  assert.equal(releaseNotes({ channel: 'stable', signed: false }), `Download it from https://perland.online/launcher.${warning}`);
  assert.throws(() => releaseNotes({ channel: 'nightly', signed: true }), /channel/);
});

test('the signing command tells plugins, the installer and our own binaries apart', () => {
  const raw = String.raw;
  assert.equal(signRole(raw`D:\a\perland-launcher\perland-launcher\target\release\perland-launcher.exe`), 'binary');
  assert.equal(signRole(raw`C:\Users\RUNNER~1\AppData\Local\Temp\nst1234.tmp`), 'binary');
  assert.equal(signRole(raw`D:\a\p\target\release\nsis\x64\Plugins\x86-unicode\System.dll`), 'plugin');
  assert.equal(signRole('D:/a/p/target/release/nsis/x64/plugins/X86-UNICODE/additional/nsis_tauri_utils.dll'), 'plugin');
  assert.equal(signRole(raw`D:\a\p\target\release\bundle\nsis\Perland Launcher_0.2.0-beta.3_x64-setup.exe`), 'installer');
  assert.equal(signRole('/home/runner/work/p/target/release/bundle/nsis/Perland Launcher_0.2.0_x64-setup.exe'), 'installer');
  assert.equal(signRole(raw`C:\Users\RUNNER~1\AppData\Local\Temp\nstA0F3.tmp`), 'binary');
});

test('the signing command refuses a path it does not recognise instead of signing it', () => {
  const raw = String.raw;
  // A -setup.exe outside bundle/nsis is not the final installer, and nothing else is ours.
  assert.throws(() => signRole(raw`D:\a\p\target\release\other-setup.exe`), /other-setup\.exe.*not a file this build signs/s);
  assert.throws(() => signRole(raw`D:\a\p\target\release\some-sidecar.exe`), /not a file this build signs/);
  assert.throws(() => signRole(raw`C:\Users\RUNNER~1\AppData\Local\Temp\other.tmp`), /not a file this build signs/);
  assert.throws(() => signRole(raw`D:\a\p\target\release\perland-launcher.exe.bak`), /not a file this build signs/);
});

test('the apply pass replaces our binaries, skips the rest, and refuses unknown ones', () => {
  const signed = new Set(['aa11']);
  assert.equal(planApply({ role: 'binary', hash: 'aa11', signed }), 'replace');
  assert.equal(planApply({ role: 'plugin', hash: 'ffff', signed }), 'skip');
  assert.equal(planApply({ role: 'installer', hash: 'ffff', signed }), 'skip');
  assert.throws(() => planApply({ role: 'binary', hash: 'bb22', signed }), /bb22.*not reproducible/s);
});

// makensis ignores the exit code of the uninstaller's sign command, so each pass is checked from the ledger.
test('the ledger check passes a clean collect and a clean apply', () => {
  const collect = ['sign-collect binary aa11 collected C:/t/perland-launcher.exe', 'sign-collect plugin ff00 skipped C:/x/Plugins/x86-unicode/System.dll', 'sign-collect binary bb22 collected C:/Temp/nst1.tmp'];
  assert.doesNotThrow(() => checkSignLedger({ pass: 'collect', lines: collect, signedHashes: [] }));
  const apply = ['sign-apply binary aa11 replaced C:/t/perland-launcher.exe', 'sign-apply binary bb22 replaced C:/Temp/nst2.tmp', 'sign-apply installer cc33 skipped C:/b/x-setup.exe'];
  assert.doesNotThrow(() => checkSignLedger({ pass: 'apply', lines: apply, signedHashes: ['aa11', 'bb22'] }));
});

test('the ledger check fails on a swallowed error, a missing uninstaller, or a signed file never applied', () => {
  const errored = ['sign-apply binary aa11 replaced C:/t/a.exe', 'sign-apply binary dd44 error C:/Temp/nst2.tmp'];
  assert.throws(() => checkSignLedger({ pass: 'apply', lines: errored, signedHashes: ['aa11'] }), /dd44/);
  assert.throws(() => checkSignLedger({ pass: 'collect', lines: ['sign-collect binary aa11 collected C:/t/a.exe'], signedHashes: [] }), /collected 1/);
  assert.throws(() => checkSignLedger({ pass: 'collect', lines: [], signedHashes: [] }), /collected 0/);
  assert.throws(() => checkSignLedger({ pass: 'apply', lines: ['sign-apply binary aa11 replaced C:/t/a.exe'], signedHashes: ['aa11', 'bb22'] }), /bb22/);
});
