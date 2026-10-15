import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildManifest, channelName, findBundle, nextVersion, PLATFORMS, releaseName, setWorkspaceVersion } from './lib.mjs';

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
