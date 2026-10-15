#!/usr/bin/env node
// CLI for the release workflow. Every decision is in lib.mjs.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import {
  buildManifest,
  channelName,
  checkSignatureKeys,
  findBundle,
  nextVersion,
  planApply,
  PLATFORMS,
  releaseName,
  releaseNotes,
  setWorkspaceVersion,
  signRole,
} from './lib.mjs';

const root = new URL('../../', import.meta.url);
const cargoPath = new URL('Cargo.toml', root);
const tauriConfPath = new URL('src-tauri/tauri.conf.json', root);

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));
}

const [command, ...args] = process.argv.slice(2);
switch (command) {
  case 'version': {
    const base = /\[workspace\.package\][^[]*?\nversion = "([^"]+)"/.exec(readFileSync(cargoPath, 'utf8'))?.[1];
    const tags = execFileSync('git', ['tag', '--list', 'v*'], { encoding: 'utf8' }).split('\n').filter(Boolean);
    console.log(nextVersion({ base, channel: args[0], tags }));
    break;
  }
  case 'set-version':
    writeFileSync(cargoPath, setWorkspaceVersion(readFileSync(cargoPath, 'utf8'), args[0]));
    break;
  case 'stage': {
    // Copy each built installer and its .sig to outDir under its release name.
    const [version, inDir, outDir] = args;
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
    break;
  }
  case 'manifest': {
    const [version, dir, baseUrl, out, channel] = args;
    const signatures = Object.fromEntries(
      PLATFORMS.map((p) => [p, readFileSync(join(dir, `${releaseName(p, version)}.sig`), 'utf8').trim()]),
    );
    // A .sig from a key other than the committed pubkey would be rejected by every installed launcher.
    checkSignatureKeys(JSON.parse(readFileSync(tauriConfPath, 'utf8')).plugins?.updater?.pubkey, signatures);
    const notes = releaseNotes({ channel, signed: process.env.SIGNED === 'true' });
    const manifest = buildManifest({ version, notes, pubDate: new Date().toISOString(), signatures, baseUrl });
    writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
    break;
  }
  case 'notes':
    console.log(releaseNotes({ channel: args[0], signed: process.env.SIGNED === 'true' }));
    break;
  case 'channel-copies': {
    const [version, channel, dir, outDir] = args;
    mkdirSync(outDir, { recursive: true });
    for (const platform of PLATFORMS) {
      copyFileSync(join(dir, releaseName(platform, version)), join(outDir, channelName(platform, channel)));
    }
    break;
  }
  case 'sign-collect':
  case 'sign-apply': {
    // Tauri's bundle.windows.signCommand in the two-pass Windows build (signing spec §3.2–§3.3).
    const [file] = args;
    const dir = process.env.SIGN_DIR;
    if (!dir) throw new Error('SIGN_DIR is not set');
    const role = signRole(file);
    const hash = createHash('sha256').update(readFileSync(file)).digest('hex');
    if (command === 'sign-collect') {
      if (role === 'binary') {
        mkdirSync(join(dir, 'unsigned'), { recursive: true });
        copyFileSync(file, join(dir, 'unsigned', `${hash}.exe`));
      }
    } else {
      const signedDir = join(dir, 'signed');
      const signed = new Set(existsSync(signedDir) ? readdirSync(signedDir).map((n) => n.replace(/\.exe$/i, '')) : []);
      if (planApply({ role, hash, signed }) === 'replace') copyFileSync(join(signedDir, `${hash}.exe`), file);
    }
    console.log(`${command} ${role} ${hash} ${file}`);
    break;
  }
  default:
    throw new Error(`Unknown command ${command}`);
}
