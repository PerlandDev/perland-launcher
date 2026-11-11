import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { test } from 'node:test';
import { assignVersions, baseOf, buildNotes, compareVersions, MAX_GROUPS, parseNote, releaseBody, releaseVersions } from './notes.mjs';

const ok = '{"kind":"new","en":"A.","es":"B."}';
const json = (note) => JSON.stringify({ kind: 'new', en: 'A.', es: 'B.', ...note });

test('a note parses, whatever its line endings', () => {
  assert.deepEqual(parseNote('whats-new.json', `${ok}\r\n`), { id: 'whats-new', kind: 'new', en: 'A.', es: 'B.' });
  assert.deepEqual(parseNote('a1-b2.json', '{\r\n  "kind": "fixed",\r\n  "en": "A.",\r\n  "es": "B."\r\n}\n').kind, 'fixed');
});

test('a bad slug is refused, and the error names the file', () => {
  assert.throws(() => parseNote('Whats_New.json', ok), /^Error: release-notes\/Whats_New\.json: /);
  assert.throws(() => parseNote('a--b.json', ok), /release-notes\/a--b\.json: /);
  assert.throws(() => parseNote('a.txt', ok), /release-notes\/a\.txt: /);
});

test('the kind is one of the three', () => {
  assert.throws(() => parseNote('a.json', json({ kind: 'big' })), /kind/);
});

test('both languages are needed, and nothing else', () => {
  assert.throws(() => parseNote('a.json', '{"kind":"new","en":"A."}'), /es/);
  assert.throws(() => parseNote('a.json', json({ es: '' })), /es/);
  assert.throws(() => parseNote('a.json', json({ pr: 1 })), /pr/);
  assert.throws(() => parseNote('a.json', '[]'), /release-notes\/a\.json: /);
  assert.throws(() => parseNote('a.json', '{nope'), /release-notes\/a\.json: /);
});

test('a note is one plain line', () => {
  assert.throws(() => parseNote('a.json', json({ en: 'A\nB' })), /en/);
  assert.throws(() => parseNote('a.json', json({ es: 'A\tB' })), /es/);
  assert.throws(() => parseNote('a.json', json({ en: ' A.' })), /en/);
  assert.throws(() => parseNote('a.json', json({ en: 'See https://x.y' })), /en/);
  assert.throws(() => parseNote('a.json', json({ en: 'A <b>' })), /en/);
  assert.throws(() => parseNote('a.json', json({ es: 'Un `x`' })), /es/);
  assert.throws(() => parseNote('a.json', json({ es: '[x](y)' })), /es/);
  assert.throws(() => parseNote('a.json', json({ en: '**A**' })), /en/);
});

test('a note cannot ping a person or link an issue from the GitHub Release', () => {
  assert.throws(() => parseNote('a.json', json({ en: 'Thanks @steve.' })), /en/);
  assert.throws(() => parseNote('a.json', json({ es: 'Arregla #12.' })), /es/);
  parseNote('a.json', json({ en: 'The #1 thing you asked for @ last.' }).replace('#1', '# 1'));
});

test('140 characters pass and 141 fail, counted as a player reads them', () => {
  parseNote('a.json', json({ en: 'ñ'.repeat(140), es: '→'.repeat(140) }));
  parseNote('a.json', json({ en: '😀'.repeat(140) }));
  assert.throws(() => parseNote('a.json', json({ en: 'a'.repeat(141) })), /140/);
});

test('release versions are the CalVer tags, oldest first', () => {
  assert.deepEqual(releaseVersions(['v0.2.14', 'v2026.11.1', 'v2026.11.1-beta.2', 'v2026.11.1-beta.10', 'v2026.11.0', 'channel-beta', 'v2027.1.0', 'v2026.12.3']), [
    '2026.11.0',
    '2026.11.1-beta.2',
    '2026.11.1-beta.10',
    '2026.11.1',
    '2026.12.3',
    '2027.1.0',
  ]);
  assert.ok(compareVersions('2026.11.5-beta.1', '2026.11.5') < 0);
  assert.equal(compareVersions('2026.11.5', '2026.11.5'), 0);
});

test('a base version drops the beta part', () => {
  assert.equal(baseOf('2026.11.5-beta.2'), '2026.11.5');
  assert.equal(baseOf('2026.11.5'), '2026.11.5');
});

const rel = (version, ...ids) => ({ version, ids });
const since = (releases, current) => Object.fromEntries(assignVersions({ releases, current }));

test('with no tags, every note belongs to the build', () => {
  assert.deepEqual(since([], { version: '2026.11.5', ids: ['a'] }), { a: '2026.11.5' });
});

test('a note belongs to the first release that had it', () => {
  assert.deepEqual(
    since([rel('2026.11.4'), rel('2026.11.5-beta.1', 'a'), rel('2026.11.5', 'a', 'b')], { version: '2026.11.6', ids: ['a', 'b', 'c'] }),
    { a: '2026.11.5-beta.1', b: '2026.11.5', c: '2026.11.6' },
  );
});

test('a beta from a branch that merged after the stable is dated to the next release', () => {
  assert.deepEqual(since([rel('2026.11.5-beta.3', 'e'), rel('2026.11.5')], { version: '2026.11.6', ids: ['e'] }), { e: '2026.11.6' });
});

test('a beta from a branch that is behind main does not re-date a note that already shipped', () => {
  assert.deepEqual(since([rel('2026.11.5', 'a'), rel('2026.11.6-beta.1')], { version: '2026.11.6', ids: ['a'] }), { a: '2026.11.5' });
  assert.deepEqual(since([rel('2026.11.5-beta.1', 'a'), rel('2026.11.5-beta.2'), rel('2026.11.5', 'a')], { version: '2026.11.6', ids: ['a'] }), { a: '2026.11.5-beta.1' });
});

test('a note removed and added again counts from its return', () => {
  assert.deepEqual(since([rel('2026.11.3', 'a'), rel('2026.11.4')], { version: '2026.11.5', ids: ['a'] }), { a: '2026.11.5' });
});

test('in a dev build, unreleased notes have no version', () => {
  assert.deepEqual(since([rel('2026.11.4', 'a')], { version: null, ids: ['a', 'b'] }), { a: '2026.11.4', b: null });
});

const note = (id, kind) => ({ id, kind, en: `${id} en`, es: `${id} es` });
const history = [
  { version: '2026.11.4', date: '2026-11-10', ids: [] },
  { version: '2026.11.5-beta.1', date: '2026-11-11', ids: ['a'] },
  { version: '2026.11.5', date: '2026-11-12', ids: ['a', 'b'] },
];
const notesNow = [note('a', 'fixed'), note('b', 'new'), note('c', 'improved')];
const built = buildNotes({ releases: history, current: { version: '2026.11.6', date: '2026-11-20', notes: notesNow } });

test('the notes file lists every release and groups notes by base version', () => {
  assert.deepEqual(built.versions, ['2026.11.4', '2026.11.5-beta.1', '2026.11.5', '2026.11.6']);
  assert.deepEqual(
    built.groups.map((g) => [g.base, g.date, g.notes.map((n) => `${n.id}@${n.since}`)]),
    [
      ['2026.11.6', '2026-11-20', ['c@2026.11.6']],
      // The stable's day, not its beta's; new before fixed.
      ['2026.11.5', '2026-11-12', ['b@2026.11.5', 'a@2026.11.5-beta.1']],
    ],
  );
  assert.deepEqual(built.groups[0].notes[0], { id: 'c', kind: 'improved', since: '2026.11.6', en: 'c en', es: 'c es' });
});

test('a dev build puts unreleased notes under no version', () => {
  const dev = buildNotes({ releases: history, current: { version: null, date: '2026-11-20', notes: notesNow } });
  assert.deepEqual(dev.versions, ['2026.11.4', '2026.11.5-beta.1', '2026.11.5']);
  assert.deepEqual(
    dev.groups.map((g) => [g.base, g.date]),
    [
      [null, '2026-11-20'],
      ['2026.11.5', '2026-11-12'],
    ],
  );
});

test('a group still in beta takes its newest beta day', () => {
  const beta = buildNotes({ releases: history.slice(0, 2), current: { version: '2026.11.5-beta.2', date: '2026-11-13', notes: [note('a', 'fixed'), note('b', 'new')] } });
  assert.deepEqual(
    beta.groups.map((g) => [g.base, g.date, g.notes.map((n) => n.id)]),
    [['2026.11.5', '2026-11-13', ['b', 'a']]],
  );
});

test('the notes file keeps the newest ten versions that have notes', () => {
  const releases = Array.from({ length: 12 }, (_, i) => ({ version: `2026.10.${i}`, date: '2026-10-01', ids: Array.from({ length: i + 1 }, (_, j) => `n${j}`) }));
  const notes = Array.from({ length: 12 }, (_, j) => note(`n${j}`, 'new'));
  const file = buildNotes({ releases, current: { version: '2026.11.0', date: '2026-11-01', notes } });
  assert.equal(MAX_GROUPS, 10);
  assert.deepEqual(
    file.groups.map((g) => g.base),
    ['2026.10.11', '2026.10.10', '2026.10.9', '2026.10.8', '2026.10.7', '2026.10.6', '2026.10.5', '2026.10.4', '2026.10.3', '2026.10.2'],
  );
  assert.equal(file.versions.length, 13);
});

test("a stable release's description holds every note of its base, in both languages", () => {
  const file = buildNotes({ releases: history.slice(0, 2), current: { version: '2026.11.5', date: '2026-11-12', notes: [note('a', 'fixed'), note('b', 'new')] } });
  assert.equal(
    releaseBody({ notes: file, version: '2026.11.5', footer: 'Footer.' }),
    "## What's new\n\n**New**\n- b en\n\n**Fixed**\n- a en\n\n## Novedades\n\n**Nuevo**\n- b es\n\n**Arreglos**\n- a es\n\n---\nFooter.\n",
  );
});

test("a beta's description holds only what that beta added", () => {
  const file = buildNotes({ releases: history.slice(0, 2), current: { version: '2026.11.5-beta.2', date: '2026-11-13', notes: [note('a', 'fixed'), note('b', 'improved')] } });
  assert.equal(releaseBody({ notes: file, version: '2026.11.5-beta.2', footer: 'Footer.' }), "## What's new\n\n**Improved**\n- b en\n\n## Novedades\n\n**Mejoras**\n- b es\n\n---\nFooter.\n");
});

test('a release with no notes says so in both languages', () => {
  const file = buildNotes({ releases: history, current: { version: '2026.11.6', date: '2026-11-20', notes: [note('a', 'fixed'), note('b', 'new')] } });
  assert.equal(
    releaseBody({ notes: file, version: '2026.11.6', footer: 'Footer.' }),
    "No changes you'd notice. Fixes under the hood.\n\nSin cambios que vayas a notar. Arreglos internos.\n\n---\nFooter.\n",
  );
});

test('every note in release-notes/ is valid', () => {
  const dir = new URL('../../release-notes/', import.meta.url);
  const files = readdirSync(dir);
  assert.ok(files.includes('whats-new.json'));
  for (const file of files) parseNote(file, readFileSync(new URL(file, dir), 'utf8'));
});
