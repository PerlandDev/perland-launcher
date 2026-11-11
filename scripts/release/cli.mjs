#!/usr/bin/env node
// CLI for the release workflow. Every decision is in lib.mjs.
import { execFileSync } from 'node:child_process';
import { appendFileSync, copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  buildManifest,
  channelName,
  checkSignatureKeys,
  checkSignLedger,
  ciGate,
  clientIdGate,
  findBundle,
  findSetup,
  hasAuthenticodeSignature,
  nextVersion,
  notesFooter,
  PLATFORMS,
  releaseName,
  setupChannelName,
  setupReleaseName,
  setWorkspaceVersion,
  signRole,
} from './lib.mjs';
import { buildNotes, parseNote, releaseBody, releaseVersions } from './notes.mjs';

const root = new URL('../../', import.meta.url);
const cargoPath = new URL('Cargo.toml', root);
const tauriConfPath = new URL('src-tauri/tauri.conf.json', root);
const notesDir = new URL('release-notes/', root);

// Under every release's notes. The feed (latest.json) carries the same text. SIGNED=true (Windows
// code signing is on) drops the unsigned-installer warning.
const NOTES_FOOTER = notesFooter({ signed: process.env.SIGNED === 'true' });

const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();

/** The notes file for a build of `version` (null: a dev build, whose unreleased notes get no
 *  version). The releases are the CalVer tags that are ancestors of HEAD; a note's version comes
 *  from which of them have its file. A real version needs every tag, so a shallow clone is refused. */
function collectNotes(version) {
  if (version !== null && git('rev-parse', '--is-shallow-repository') === 'true') {
    throw new Error('Release notes need the full history and tags (this clone is shallow)');
  }
  const notes = readdirSync(notesDir)
    .sort()
    .map((file) => parseNote(file, readFileSync(new URL(file, notesDir), 'utf8')));
  let releases = [];
  try {
    const tags = git('tag', '--merged', 'HEAD', '--list', 'v*').split('\n').filter(Boolean);
    releases = releaseVersions(tags).map((release) => ({
      version: release,
      date: git('log', '-1', '--format=%cs', `v${release}`),
      ids: git('ls-tree', '--name-only', `v${release}`, 'release-notes/')
        .split('\n')
        .filter((path) => path.endsWith('.json'))
        .map((path) => path.slice('release-notes/'.length, -'.json'.length)),
    }));
  } catch (error) {
    // No Git or no history here (a source archive): a dev build then shows every note as unreleased.
    if (version !== null) throw error;
  }
  return buildNotes({ releases, current: { version, date: new Date().toISOString().slice(0, 10), notes } });
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));
}

const [command, ...args] = process.argv.slice(2);
switch (command) {
  case 'version': {
    const tags = execFileSync('git', ['tag', '--list', 'v*'], { encoding: 'utf8' }).split('\n').filter(Boolean);
    console.log(nextVersion({ channel: args[0], tags, now: new Date() }));
    break;
  }
  case 'ci-check': {
    // Needs GH_TOKEN (with actions: read) and GH_REPO.
    const [sha, mode] = args;
    const json = execFileSync('gh', ['run', 'list', '--workflow', 'ci.yml', '--commit', sha, '--json', 'status,conclusion', '--limit', '100'], {
      encoding: 'utf8',
    });
    const { level, message } = ciGate(JSON.parse(json), { publish: mode === 'publish' });
    console.log(level === 'ok' ? message : `::${level}::${message}`);
    if (level === 'error') process.exitCode = 1;
    break;
  }
  case 'client-id-check': {
    // The value comes from the environment (PERLAND_MS_CLIENT_ID), never the command line.
    const { level, message } = clientIdGate(process.env.PERLAND_MS_CLIENT_ID);
    console.log(level === 'ok' ? message : `::${level}::${message}`);
    if (level === 'error') process.exitCode = 1;
    break;
  }
  case 'set-version':
    writeFileSync(cargoPath, setWorkspaceVersion(readFileSync(cargoPath, 'utf8'), args[0]));
    break;
  case 'stage': {
    // Copy each built installer and its .sig to outDir under its release name, and the setup app
    // (an optional fourth argument, Windows only; it has no .sig: the updater never fetches it).
    const [version, inDir, outDir, setup] = args;
    mkdirSync(outDir, { recursive: true });
    const paths = walk(inDir);
    let staged = 0;
    for (const platform of PLATFORMS) {
      const bundle = findBundle(paths, platform);
      if (!bundle) continue;
      copyFileSync(bundle, join(outDir, releaseName(platform, version)));
      copyFileSync(`${bundle}.sig`, join(outDir, `${releaseName(platform, version)}.sig`));
      console.log(`${relative(inDir, bundle)} -> ${releaseName(platform, version)}`);
      staged += 1;
    }
    if (staged === 0) throw new Error(`No installers under ${inDir}`);
    if (setup) {
      if (!findSetup([setup])) throw new Error(`${setup} is not perland-setup.exe`);
      copyFileSync(setup, join(outDir, setupReleaseName(version)));
      console.log(`${setup} -> ${setupReleaseName(version)}`);
    }
    break;
  }
  case 'manifest': {
    // The optional fifth argument is the release's description (`notes-body`).
    const [version, dir, baseUrl, out, body] = args;
    const signatures = Object.fromEntries(
      PLATFORMS.map((p) => [p, readFileSync(join(dir, `${releaseName(p, version)}.sig`), 'utf8').trim()]),
    );
    // A .sig from a key other than the committed pubkey would be rejected by every installed launcher.
    checkSignatureKeys(JSON.parse(readFileSync(tauriConfPath, 'utf8')).plugins?.updater?.pubkey, signatures);
    const notes = body ? readFileSync(body, 'utf8').trim() : NOTES_FOOTER;
    const manifest = buildManifest({ version, notes, pubDate: new Date().toISOString(), signatures, baseUrl });
    writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
    break;
  }
  case 'notes': {
    // `notes <version|next> <out.json|->`: what the launcher bundles. `next` is a dev build.
    const [version, out] = args;
    if (!version || !out) throw new Error('Usage: notes <version|next> <out.json|->');
    const json = `${JSON.stringify(collectNotes(version === 'next' ? null : version), null, 2)}\n`;
    if (out === '-') process.stdout.write(json);
    else writeFileSync(out, json);
    break;
  }
  case 'notes-body': {
    // `notes-body <notes.json> <version> <out.md>`: the GitHub Release's description.
    const [file, version, out] = args;
    if (!file || !version || !out) throw new Error('Usage: notes-body <notes.json> <version> <out.md>');
    writeFileSync(out, releaseBody({ notes: JSON.parse(readFileSync(file, 'utf8')), version, footer: NOTES_FOOTER }));
    break;
  }
  case 'channel-copies': {
    const [version, channel, dir, outDir] = args;
    mkdirSync(outDir, { recursive: true });
    for (const platform of PLATFORMS) {
      copyFileSync(join(dir, releaseName(platform, version)), join(outDir, channelName(platform, channel)));
    }
    if (existsSync(join(dir, setupReleaseName(version)))) {
      copyFileSync(join(dir, setupReleaseName(version)), join(outDir, setupChannelName(channel)));
    }
    break;
  }
  case 'sign': {
    // Tauri's bundle.windows.signCommand: signs one file in place with the Certum cloud certificate
    // through ssign, which reads CERTUM_EMAIL and CERTUM_OTP from the environment (never pass them as
    // arguments). makensis ignores this command's exit code for the uninstaller, so every call also
    // goes to $SIGN_DIR/ledger.log, which `sign-check` reads after the build.
    const [file] = args;
    const dir = process.env.SIGN_DIR;
    if (!dir) throw new Error('SIGN_DIR is not set');
    mkdirSync(dir, { recursive: true });
    const role = signRole(file);
    let outcome = 'error';
    try {
      if (hasAuthenticodeSignature(readFileSync(file))) {
        // ssign refuses a signed file. Only NSIS plugins may come signed; sign-check fails on ours.
        outcome = 'already-signed';
      } else {
        execFileSync(process.env.SSIGN || 'ssign', ['-n', 'Perland Launcher', '-u', 'https://perland.online/launcher', file], {
          stdio: 'inherit',
        });
        if (!hasAuthenticodeSignature(readFileSync(file))) throw new Error(`ssign left ${file} unsigned`);
        outcome = 'signed';
      }
    } finally {
      appendFileSync(join(dir, 'ledger.log'), `${role} ${outcome} ${file}\n`);
      console.log(`${role} ${outcome} ${file}`);
    }
    break;
  }
  case 'sign-check': {
    // After the Windows build: print the ledger and fail on anything makensis swallowed.
    const dir = process.env.SIGN_DIR;
    if (!dir) throw new Error('SIGN_DIR is not set');
    const ledgerPath = join(dir, 'ledger.log');
    const lines = existsSync(ledgerPath) ? readFileSync(ledgerPath, 'utf8').split('\n') : [];
    console.log(lines.join('\n'));
    checkSignLedger(lines);
    break;
  }
  case 'sign-config': {
    // A --config file for `tauri build` that sets `sign` above as the signCommand. A file, not inline
    // JSON, so no shell (pnpm's cmd shim included) gets to re-quote it or touch `%1`.
    const [out] = args;
    const self = fileURLToPath(import.meta.url).replaceAll('\\', '/');
    const config = { bundle: { windows: { signCommand: { cmd: 'node', args: [self, 'sign', '%1'] } } } };
    writeFileSync(out, `${JSON.stringify(config, null, 2)}\n`);
    break;
  }
  default:
    throw new Error(`Unknown command ${command}`);
}
