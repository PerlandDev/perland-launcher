// Refreshes the mock skin catalog (ui/src/ipc/mock-catalog) from a PerlandEngine checkout:
//
//   node ui/scripts/sync-mock-catalog.mjs <engine>/internal/skincatalog/data
//
// Copies manifest.json as it is and exactly the PNGs it names (art, masks, thumbnails, paintings and thumbnail
// sheets), each checked against the SHA-256 in its name, and removes the PNGs it no longer names. Nothing is written
// when a file is missing or doesn't match. crates/launcher-core/tests/fixtures/skin-catalog-manifest.json is a copy
// of the same manifest for the Rust tests: copy it by hand when those should follow.
import { createHash } from 'node:crypto';
import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HASH = /^[0-9a-f]{64}$/;
const target = resolve(dirname(fileURLToPath(import.meta.url)), '../src/ipc/mock-catalog');

/** Every file a manifest names, as the launcher's `Manifest::hashes` counts them. */
function namedFiles(manifest) {
  const named = new Set();
  for (const piece of manifest.pieces) {
    named.add(piece.thumb);
    for (const files of Object.values(piece.files)) named.add(files.art).add(files.mask);
  }
  for (const look of manifest.looks) {
    named.add(look.thumb);
    if (look.painting) named.add(look.painting);
  }
  // A list with no entries has no sheet: an empty file.
  for (const sheet of Object.values(manifest.sheets ?? {})) if (sheet.file) named.add(sheet.file);
  return [...named].sort();
}

async function main() {
  const [source] = process.argv.slice(2);
  if (!source) throw new Error('usage: node ui/scripts/sync-mock-catalog.mjs <engine>/internal/skincatalog/data');
  const manifestBytes = await readFile(join(source, 'manifest.json'));
  const named = namedFiles(JSON.parse(manifestBytes.toString('utf8')));
  const files = new Map();
  for (const hash of named) {
    if (!HASH.test(hash)) throw new Error(`the manifest names a file that is no hash: ${hash}`);
    const bytes = await readFile(join(source, `${hash}.png`));
    if (createHash('sha256').update(bytes).digest('hex') !== hash) throw new Error(`${hash}.png doesn't match its name`);
    files.set(hash, bytes);
  }
  await mkdir(target, { recursive: true });
  let removed = 0;
  for (const name of await readdir(target)) {
    if (name.endsWith('.png') && !files.has(name.slice(0, -'.png'.length))) {
      await rm(join(target, name));
      removed += 1;
    }
  }
  for (const [hash, bytes] of files) await writeFile(join(target, `${hash}.png`), bytes);
  await writeFile(join(target, 'manifest.json'), manifestBytes);
  console.log(`mock catalog: ${files.size} files, ${removed} removed`);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
