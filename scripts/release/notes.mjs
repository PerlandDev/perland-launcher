// Release notes for players (docs/superpowers/specs/2026-11-10-whats-new-design.md). Pure: cli.mjs
// reads the files and the tags. Tested with node:test.
import { CALVER } from './lib.mjs';

export const KINDS = ['new', 'improved', 'fixed'];
/** The longest a note may be in each language, in characters as a player reads them. */
export const MAX_CHARS = 140;
/** The base versions the launcher carries notes for. */
export const MAX_GROUPS = 10;

const SLUG = /^([a-z0-9]+(?:-[a-z0-9]+)*)\.json$/;
const CONTROL = /[\u0000-\u001f\u007f]/;
const LINK = /https?:\/\//i;
const MARKUP = /[<>[\]*`]/;
// GitHub turns these into a ping and an issue link in the Release's description.
const MENTION = /@\w|#\d/;

/** Why `text` can't be a note's sentence, or null when it can. */
function textProblem(text) {
  if (typeof text !== 'string' || text === '') return 'is missing';
  if (CONTROL.test(text)) return 'must be one line, with no tabs or line breaks';
  if (text !== text.trim()) return 'must not start or end with a space';
  if (LINK.test(text)) return 'must not hold a link';
  if (MARKUP.test(text)) return 'must be plain text (none of < > [ ] * `)';
  if (MENTION.test(text)) return 'must not hold an @name or a #number (GitHub would link them)';
  if ([...text].length > MAX_CHARS) return `is longer than ${MAX_CHARS} characters`;
  return null;
}

/** One note from `release-notes/<fileName>`. Throws with the file's name on anything a player shouldn't get. */
export function parseNote(fileName, text) {
  const fail = (why) => {
    throw new Error(`release-notes/${fileName}: ${why}`);
  };
  const slug = SLUG.exec(fileName);
  if (!slug) fail('the name must be lowercase words and digits joined by hyphens, ending in .json');
  let raw;
  try {
    raw = JSON.parse(text);
  } catch (error) {
    fail(`not JSON (${error.message})`);
  }
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) fail('must be an object with kind, en and es');
  const extra = Object.keys(raw).filter((key) => !['kind', 'en', 'es'].includes(key));
  if (extra.length > 0) fail(`unknown key ${extra.join(', ')} (a note has only kind, en and es)`);
  if (!KINDS.includes(raw.kind)) fail(`kind must be one of ${KINDS.join(', ')}`);
  for (const lang of ['en', 'es']) {
    const problem = textProblem(raw[lang]);
    if (problem) fail(`${lang} ${problem}`);
  }
  return { id: slug[1], kind: raw.kind, en: raw.en, es: raw.es };
}

const VERSION = /^(\d{4})\.(\d+)\.(\d+)(?:-beta\.(\d+))?$/;

function parts(version) {
  const m = VERSION.exec(version);
  if (!m) throw new Error(`Not a release version: "${version}"`);
  // A stable sorts after every beta of its base.
  return [+m[1], +m[2], +m[3], m[4] === undefined ? Infinity : +m[4]];
}

/** Orders `YYYY.M.MICRO` and `YYYY.M.MICRO-beta.N`: a beta comes before its stable. */
export function compareVersions(a, b) {
  const x = parts(a);
  const y = parts(b);
  for (let i = 0; i < x.length; i += 1) {
    if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
  }
  return 0;
}

/** The versions among `tags` (CalVer only; old `v0.x` tags don't count), oldest first. */
export function releaseVersions(tags) {
  return tags
    .filter((tag) => CALVER.test(tag))
    .map((tag) => tag.slice(1))
    .sort(compareVersions);
}

export function baseOf(version) {
  return version.replace(/-beta\.\d+$/, '');
}

/** The version each of `current.ids` first shipped in: the oldest release such that it and every
 *  later stable, up to the build, have the note. `releases` is oldest first. A note no release has
 *  belongs to the build (`current.version`, null in a dev build). Only a stable without the note
 *  ends the run: a beta may come from a branch that is behind main, and its lacking a note that
 *  already shipped must not make that note new again. */
export function assignVersions({ releases, current }) {
  const since = new Map();
  for (const id of current.ids) {
    let version = current.version;
    for (let i = releases.length - 1; i >= 0; i -= 1) {
      if (releases[i].ids.includes(id)) version = releases[i].version;
      else if (releases[i].version === baseOf(releases[i].version)) break;
    }
    since.set(id, version);
  }
  return since;
}

/** What the launcher bundles: every release in order (so it can compare versions by position),
 *  and the notes grouped by base version, newest first, unreleased ones before all. */
export function buildNotes({ releases, current, cap = MAX_GROUPS }) {
  const since = assignVersions({ releases, current: { version: current.version, ids: current.notes.map((n) => n.id) } });
  const all = current.version === null ? releases : [...releases, { version: current.version, date: current.date }];
  const byBase = new Map();
  for (const note of current.notes) {
    const version = since.get(note.id);
    const base = version === null ? null : baseOf(version);
    if (!byBase.has(base)) byBase.set(base, []);
    byBase.get(base).push({ id: note.id, kind: note.kind, since: version, en: note.en, es: note.es });
  }
  const dateOf = (base) => (base === null ? current.date : all.findLast((r) => baseOf(r.version) === base).date);
  const groups = [...byBase.entries()]
    .sort(([a], [b]) => (a === null ? -1 : b === null ? 1 : compareVersions(b, a)))
    .slice(0, cap)
    .map(([base, notes]) => ({
      base,
      date: dateOf(base),
      notes: notes.sort((a, b) => KINDS.indexOf(a.kind) - KINDS.indexOf(b.kind) || (a.id < b.id ? -1 : 1)),
    }));
  return { versions: all.map((r) => r.version), groups };
}

const HEADINGS = {
  en: { title: "What's new", new: 'New', improved: 'Improved', fixed: 'Fixed' },
  es: { title: 'Novedades', new: 'Nuevo', improved: 'Mejoras', fixed: 'Arreglos' },
};
const NO_NOTES = "No changes you'd notice. Fixes under the hood.\n\nSin cambios que vayas a notar. Arreglos internos.";

/** The GitHub Release description of `version`: a stable lists every note of its base version, a
 *  beta only what that beta added. English, then Spanish, then `footer`. */
export function releaseBody({ notes, version, footer }) {
  const group = notes.groups.find((g) => g.base === baseOf(version));
  const mine = (group?.notes ?? []).filter((n) => version === baseOf(version) || n.since === version);
  const section = (lang) =>
    [
      `## ${HEADINGS[lang].title}`,
      ...KINDS.filter((kind) => mine.some((n) => n.kind === kind)).map(
        (kind) =>
          `**${HEADINGS[lang][kind]}**\n${mine
            .filter((n) => n.kind === kind)
            .map((n) => `- ${n[lang]}`)
            .join('\n')}`,
      ),
    ].join('\n\n');
  const body = mine.length === 0 ? NO_NOTES : `${section('en')}\n\n${section('es')}`;
  return `${body}\n\n---\n${footer}\n`;
}
