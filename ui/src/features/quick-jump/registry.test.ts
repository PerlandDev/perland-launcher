import { afterEach, describe, expect, it } from 'vitest';
import i18n from '../../i18n';
import listingFx from '../../ipc/contract/instance_listing.json';
import type { Instance, InstanceListing } from '../../ipc/schema';
import { buildEntries, suggested, type Entry } from './registry';

const listing = JSON.parse(JSON.stringify(listingFx)) as InstanceListing;
const asalith = listing.instances.find((i) => i.id === 'asalith-fields')!;
const flat = listing.instances.find((i) => i.id === 'creative-flat')!;
const t = i18n.t.bind(i18n);

const build = (over: Partial<Parameters<typeof buildEntries>[0]> = {}) =>
  buildEntries({ t, instances: [asalith, flat], current: 'asalith-fields', running: [], ownTheme: false, ...over });
const labelsOf = (entries: Entry[], group: string) => entries.filter((e) => e.group.id === group).map((e) => e.label);

afterEach(() => void i18n.changeLanguage('en'));

describe('buildEntries', () => {
  it("lists the launcher's places by their own names", () => {
    expect(labelsOf(build(), 'places')).toEqual([
      'Home',
      'Skins',
      'Servers',
      'Mods & more',
      'Medals and achievements',
      'Your profile',
      'Personalize',
      'Settings',
      'New instance',
    ]);
  });

  it('lists the current instance first, then every other one, each under its name', () => {
    const groups = [...new Set(build({ current: 'creative-flat' }).map((e) => e.group.label))];
    expect(groups.indexOf('Creative Flat')).toBeLessThan(groups.indexOf('Asalith Fields'));
    expect(build({ current: 'creative-flat' }).filter((e) => e.current).every((e) => e.group.label === 'Creative Flat')).toBe(true);
  });

  it("offers an instance's screens, and Find mods and Find shaders only with a loader", () => {
    expect(labelsOf(build(), 'instance:asalith-fields')).toEqual([
      'Home',
      'Mods & packs',
      'Resource packs',
      'Shaders',
      'Instance settings',
      'Find mods',
      'Find resource packs',
      'Find shaders',
      'Rename',
      'Change image…',
      'Share…',
      'Open folder',
    ]);
    const vanilla = labelsOf(build(), 'instance:creative-flat');
    expect(vanilla).toContain('Find resource packs');
    expect(vanilla).not.toContain('Find mods');
    expect(vanilla).not.toContain('Find shaders');
  });

  it('never offers Delete, and leaves out what a running game refuses', () => {
    const entries = build({ running: ['asalith-fields'] });
    expect(entries.some((e) => /delete/i.test(e.label))).toBe(false);
    const labels = labelsOf(entries, 'instance:asalith-fields');
    expect(labels).not.toContain('Change image…');
    expect(labels).not.toContain('Share…');
    expect(labels).toContain('Rename');
  });

  it('does not offer Share on a server instance, which shares by its invite link', () => {
    const server: Instance = { ...flat, id: 'srv', name: 'Survival', source: { kind: 'perland', invite: 'abc' } as Instance['source'] };
    expect(labelsOf(build({ instances: [server], current: 'srv' }), 'instance:srv')).not.toContain('Share…');
  });

  it('points each result at its place', () => {
    const entries = build();
    const target = (group: string, label: string) => entries.find((e) => e.group.id === group && e.label === label)!.target;
    expect(target('instance:asalith-fields', 'Shaders')).toEqual({ kind: 'route', route: { name: 'mods', id: 'asalith-fields', tab: 'shader' } });
    expect(target('instance:asalith-fields', 'Find mods')).toEqual({ kind: 'route', route: { name: 'browse', kind: 'mod', instance: 'asalith-fields' } });
    expect(target('instance:creative-flat', 'Instance settings')).toEqual({ kind: 'route', route: { name: 'instanceSettings', id: 'creative-flat' } });
    expect(target('instance:creative-flat', 'Rename')).toEqual({ kind: 'action', instance: 'creative-flat', action: 'rename' });
    expect(target('settings', 'Language')).toEqual({ kind: 'field', route: { name: 'settings' }, find: 'Language' });
    expect(target('personalize', 'Themes')).toEqual({ kind: 'field', route: { name: 'personalize' }, find: 'Themes' });
  });

  it("lists Settings' and Personalize's fields by their labels", () => {
    expect(labelsOf(build(), 'settings')).toEqual([
      'Language',
      'Memory for new instances',
      'Graphics card',
      'Minimize while playing',
      'Data folder',
      'Launcher updates',
      'Diagnostics',
      'Privacy',
      'About',
    ]);
    expect(labelsOf(build(), 'personalize')).toEqual(['Themes', 'Customize theme', 'Button colour', 'Text font', 'Instance icons', 'Screen transitions']);
    // With the player's own theme on, Your colours shows and Button colour doesn't.
    expect(labelsOf(build({ ownTheme: true }), 'personalize')).toEqual(['Themes', 'Customize theme', 'Your colours', 'Text font', 'Instance icons', 'Screen transitions']);
  });

  it('speaks the language in use', async () => {
    await i18n.changeLanguage('es');
    const entries = build();
    expect(labelsOf(entries, 'settings')).toContain('Idioma');
    expect(labelsOf(entries, 'instance:asalith-fields')).toContain('Mods y packs');
  });
});

describe('suggested', () => {
  it("is the current instance's screens, then Settings and Personalize", () => {
    expect(suggested(build()).map((e) => `${e.group.label}: ${e.label}`)).toEqual([
      'Asalith Fields: Home',
      'Asalith Fields: Mods & packs',
      'Asalith Fields: Resource packs',
      'Asalith Fields: Shaders',
      'Asalith Fields: Instance settings',
      'Places: Personalize',
      'Places: Settings',
    ]);
  });

  it('is only Settings and Personalize without an instance', () => {
    expect(suggested(build({ instances: [], current: null })).map((e) => e.label)).toEqual(['Personalize', 'Settings']);
  });
});
