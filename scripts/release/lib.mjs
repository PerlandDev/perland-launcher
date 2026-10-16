// Pure helpers for .github/workflows/release.yml (spec §3.2–§3.4). Tested with node:test.

/** Updater manifest keys (tauri-plugin-updater looks up `<os>-<arch>-<installer>` first). */
export const PLATFORMS = ['windows-x86_64-nsis', 'linux-x86_64-appimage', 'linux-x86_64-deb'];

const SUFFIX = {
  'windows-x86_64-nsis': { release: '_x64-setup.exe', channel: '-setup.exe', bundle: /_x64-setup\.exe$/ },
  'linux-x86_64-appimage': { release: '_amd64.AppImage', channel: '.AppImage', bundle: /\.AppImage$/ },
  'linux-x86_64-deb': { release: '_amd64.deb', channel: '.deb', bundle: /\.deb$/ },
};

export function nextVersion({ base, channel, tags }) {
  if (!/^\d+\.\d+\.\d+$/.test(base)) throw new Error(`The base version ${base} must be x.y.z`);
  const released = tags.includes(`v${base}`);
  if (channel === 'stable') {
    if (released) throw new Error(`v${base} is already released; bump the version on main`);
    return base;
  }
  if (channel !== 'beta') throw new Error(`Unknown channel ${channel}`);
  if (released) throw new Error(`v${base} is already stable; bump the version on main before a new beta`);
  const prefix = `v${base}-beta.`;
  const numbers = tags
    .filter((tag) => tag.startsWith(prefix))
    .map((tag) => tag.slice(prefix.length))
    .filter((n) => /^\d+$/.test(n))
    .map(Number);
  return `${base}-beta.${Math.max(0, ...numbers) + 1}`;
}

/** Set `[workspace.package] version`, leaving every other `version =` alone. */
export function setWorkspaceVersion(cargoToml, version) {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(String(version))) {
    throw new Error(`Not a version: "${version}"`);
  }
  const section = /(\[workspace\.package\][^[]*?\nversion = ")[^"]*(")/;
  if (!section.test(cargoToml)) throw new Error('No [workspace.package] version in Cargo.toml');
  return cargoToml.replace(section, `$1${version}$2`);
}

export function releaseName(platform, version) {
  return `PerlandLauncher_${version}${SUFFIX[platform].release}`;
}

export function channelName(platform, channel) {
  return `PerlandLauncher${channel === 'beta' ? '-beta' : ''}${SUFFIX[platform].channel}`;
}

export function findBundle(paths, platform) {
  return paths.find((p) => SUFFIX[platform].bundle.test(p));
}

const BASE64 = /^[A-Za-z0-9+/]+={0,2}$/;

function fromBase64(text) {
  const trimmed = text.trim();
  if (!BASE64.test(trimmed)) throw new Error('Not a minisign key or signature (not base64)');
  return Buffer.from(trimmed, 'base64');
}

/** The 8-byte key id (hex) of a Tauri updater public key or `.sig`. Both are base64 of a minisign
 *  text file whose second line is base64 of [2-byte algorithm "Ed"/"ED"][8-byte key id][...]. */
export function keyIdOf(base64Text) {
  const line = fromBase64(base64Text ?? '').toString('utf8').split(/\r?\n/)[1];
  if (!line) throw new Error('Not a minisign key or signature (no key line)');
  const bytes = fromBase64(line);
  if (bytes.length < 10 || bytes[0] !== 0x45 || (bytes[1] !== 0x64 && bytes[1] !== 0x44)) {
    throw new Error('Not a minisign key or signature (bad key line)');
  }
  return bytes.subarray(2, 10).toString('hex');
}

/** Every platform's `.sig` must come from the key whose public half is committed, or installed
 *  launchers reject the update (and every later one). */
export function checkSignatureKeys(pubkey, signatures) {
  if (!pubkey?.trim()) throw new Error('plugins.updater.pubkey in src-tauri/tauri.conf.json is empty');
  const want = keyIdOf(pubkey);
  const wrong = Object.entries(signatures).filter(([, sig]) => keyIdOf(sig) !== want);
  if (wrong.length > 0) {
    const names = wrong.map(([platform, sig]) => `${platform} (key ${keyIdOf(sig)})`).join(', ');
    throw new Error(
      `Signed with a different key than the committed pubkey (key ${want}): ${names}. ` +
        'TAURI_SIGNING_PRIVATE_KEY and plugins.updater.pubkey in src-tauri/tauri.conf.json must be one pair.',
    );
  }
}

export function buildManifest({ version, notes, pubDate, signatures, baseUrl }) {
  const platforms = {};
  for (const platform of PLATFORMS) {
    const signature = signatures[platform];
    if (!signature) throw new Error(`No updater signature for ${platform}`);
    platforms[platform] = { signature, url: `${baseUrl}/${releaseName(platform, version)}` };
  }
  return { version, notes, pub_date: pubDate, platforms };
}

const FAILED = new Set(['failure', 'timed_out', 'startup_failure']);

/** Whether CI passed on the commit being released, from `gh run list --json status,conclusion`.
 *  A publish needs a green run and no red one; a dry run only warns. */
export function ciGate(runs, { publish }) {
  const level = publish ? 'error' : 'warning';
  if (runs.some((r) => r.status === 'completed' && FAILED.has(r.conclusion))) {
    return { level, message: 'CI failed on this commit. Fix it, then release again.' };
  }
  if (runs.some((r) => r.status === 'completed' && r.conclusion === 'success')) {
    return { level: 'ok', message: 'CI passed on this commit.' };
  }
  if (runs.some((r) => r.status !== 'completed')) {
    return { level, message: 'CI is still running on this commit. Wait for it to pass, then release again.' };
  }
  if (runs.length > 0) {
    return { level, message: 'CI has not passed on this commit (its runs were cancelled or skipped). Re-run it first.' };
  }
  return { level, message: 'CI has not run on this commit. Push it to main or to a branch with a pull request, and wait for CI to pass.' };
}
