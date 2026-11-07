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

/** The one-click setup app (crates/perland-setup): a Windows download only, never in latest.json. */
export function setupReleaseName(version) {
  return `PerlandLauncher_${version}_x64-installer.exe`;
}

export function setupChannelName(channel) {
  return `PerlandLauncher${channel === 'beta' ? '-beta' : ''}-installer.exe`;
}

export function findSetup(paths) {
  return paths.find((p) => /(^|[\\/])perland-setup\.exe$/.test(p));
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

/** Which Microsoft app the release signs in with. Without the repository variable PERLAND_MS_CLIENT_ID it is the
 *  Minecraft Launcher's public client, which Minecraft accepts. With it, it is that Azure app, whose sign-ins Minecraft
 *  turns down until Mojang approves it: a warning, so a release isn't built with it by mistake. */
export function clientIdGate(value) {
  if (typeof value === 'string' && value.trim() !== '') {
    return {
      level: 'warning',
      message:
        "PERLAND_MS_CLIENT_ID is set, so this build signs in with that Azure app. Minecraft accepts its sign-ins only once Mojang has approved it; clear the variable to sign in with the Minecraft Launcher's client.",
    };
  }
  return { level: 'ok', message: "No PERLAND_MS_CLIENT_ID: this build signs in with the Minecraft Launcher's client." };
}

/** Whether a Windows PE file already carries an Authenticode signature: its certificate table (data
 *  directory 4) is non-empty. Tauri hands the sign command NSIS plugin DLLs without checking, and ssign
 *  refuses a signed file, so the sign command skips these. */
export function hasAuthenticodeSignature(buf) {
  const notPe = () => new Error('This is not a Windows executable (no PE header)');
  if (buf.length < 0x40 || buf.toString('latin1', 0, 2) !== 'MZ') throw notPe();
  const pe = buf.readUInt32LE(0x3c);
  const optional = pe + 24;
  if (optional + 2 > buf.length || buf.toString('latin1', pe, pe + 4) !== 'PE\0\0') throw notPe();
  const magic = buf.readUInt16LE(optional);
  if (magic !== 0x10b && magic !== 0x20b) throw notPe();
  const security = optional + (magic === 0x20b ? 112 : 96) + 4 * 8;
  if (security + 8 > buf.length) throw notPe();
  return buf.readUInt32LE(security + 4) > 0;
}

/** What a file handed to the Windows sign command is: an NSIS plugin DLL, the NSIS installer, the
 *  one-click setup app, or one of our binaries (the launcher .exe and the two uninstallers). */
export function signRole(path) {
  const p = String(path).replaceAll('\\', '/').toLowerCase();
  if (p.includes('/plugins/x86-unicode/')) return 'plugin';
  if (p.includes('/bundle/nsis/') && p.endsWith('-setup.exe')) return 'installer';
  if (/(^|\/)perland-setup\.exe$/.test(p)) return 'setup';
  return 'binary';
}

const baseName = (file) => file.replaceAll('\\', '/').split('/').pop().toLowerCase();

/** Every file a signed Windows release must have signed itself, as [label, role, file name or null]. */
const MUST_SIGN = [
  ['perland-launcher.exe', 'binary', 'perland-launcher.exe'],
  ['the NSIS uninstaller', 'binary', 'uninstall.exe'],
  ['perland-uninstall.exe', 'binary', 'perland-uninstall.exe'],
  ['the NSIS installer', 'installer', null],
  ['the setup app', 'setup', 'perland-setup.exe'],
];

/** Check a Windows build's signing ledger (`<role> <outcome> <file>` per sign command call). makensis
 *  ignores the exit code of the uninstaller's sign command, so a failure there only shows up here. */
export function checkSignLedger(lines) {
  const entries = lines
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const [role, outcome, ...file] = line.split(' ');
      return { role, outcome, file: file.join(' ') };
    });
  const failed = entries.filter((e) => e.outcome !== 'signed' && e.outcome !== 'already-signed');
  if (failed.length > 0) throw new Error(`Signing failed for ${failed.map((e) => e.file).join(', ')}`);
  const missing = MUST_SIGN.filter(
    ([, role, name]) => !entries.some((e) => e.role === role && e.outcome === 'signed' && (!name || baseName(e.file) === name)),
  ).map(([label]) => label);
  if (missing.length > 0) throw new Error(`This build never signed: ${missing.join(', ')}`);
}
