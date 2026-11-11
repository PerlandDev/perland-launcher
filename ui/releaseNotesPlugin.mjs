// `virtual:release-notes`: the notes What's new shows (docs/superpowers/specs/2026-11-10-whats-new-design.md).
// A release bundles the file the Release workflow worked out (PERLAND_RELEASE_NOTES). Any other build asks
// the release CLI, which reads release-notes/ and the tags in this checkout and files unreleased notes
// under no version. Plain JavaScript: vite.config.ts is type-checked without Node's types.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const ID = 'virtual:release-notes';

export function releaseNotes() {
  return {
    name: 'perland-release-notes',
    resolveId: (source) => (source === ID ? `\0${ID}` : null),
    load(resolved) {
      if (resolved !== `\0${ID}`) return null;
      const file = process.env.PERLAND_RELEASE_NOTES;
      const json = file
        ? readFileSync(file, 'utf8')
        : execFileSync(process.execPath, [fileURLToPath(new URL('../scripts/release/cli.mjs', import.meta.url)), 'notes', 'next', '-'], { encoding: 'utf8' });
      return `export default ${JSON.stringify(JSON.parse(json))};`;
    },
  };
}
