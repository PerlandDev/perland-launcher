// Everything Go to can reach, built from data: the launcher's places, each instance's screens and safe actions, and the
// fields of Settings and Personalize. Every label is the i18n key the place itself shows, so a result always reads as
// the thing it opens, in either language, and a renamed label renames its result.
import type { TFunction } from 'i18next';
import type { ComponentType, SVGProps } from 'react';
import type { Instance } from '../../ipc/schema';
import type { Route } from '../../state/route';
import {
  Brush,
  Download,
  Folder,
  Gear,
  Image,
  InfoBox,
  Languages,
  MemoryStick,
  Minus,
  Package,
  Pencil,
  Play,
  Plus,
  Search,
  Server,
  SettingsCog,
  Share,
  Shield,
  Shirt,
  Square,
  Terminal,
  Trophy,
  User,
} from '../../ui/Icon';
import { sessionKind } from '../browse/session';
import { isServer } from '../home/labels';
import { TAB_GLYPHS } from '../mods/tabGlyphs';

export type IconType = ComponentType<SVGProps<SVGSVGElement>>;

/** What can be done to an instance from Go to. Never Delete: a search is no place to lose a world. */
export type InstanceAction = 'rename' | 'image' | 'share' | 'openFolder';

export type Target =
  | { kind: 'route'; route: Route }
  /** A field on a screen, found by the label the screen shows for it. */
  | { kind: 'field'; route: Route; find: string }
  | { kind: 'action'; instance: string; action: InstanceAction };

export interface Entry {
  id: string;
  group: { id: string; label: string };
  label: string;
  context: string[];
  /** One of the instance the player is on. */
  current: boolean;
  icon: IconType;
  target: Target;
}

export interface Sources {
  t: TFunction;
  /** The healthy instances, in the rail's order. */
  instances: readonly Instance[];
  /** The instance the player is on (the route's, else the last one opened). */
  current: string | null;
  /** Instances with a game or an install running. */
  running: readonly string[];
  /** The player's own theme is on: Personalize shows Your colours in place of Button colour. */
  ownTheme: boolean;
}

/** The places, in the title bar's order, then the keys at its end, then New instance. */
function places(t: TFunction): Entry[] {
  const group = { id: 'places', label: t('places.label') };
  const place = (id: string, label: string, icon: IconType, route: Route): Entry => ({
    id: `place:${id}`,
    group,
    label,
    context: [group.label],
    current: false,
    icon,
    target: { kind: 'route', route },
  });
  return [
    place('home', t('quickJump.home'), Play, { name: 'home', id: null }),
    place('skins', t('titlebar.skins'), Shirt, { name: 'skins' }),
    place('servers', t('servers.open'), Server, { name: 'servers' }),
    place('browse', t('browse.open'), Package, { name: 'browse', kind: sessionKind() }),
    place('achievements', t('titlebar.achievements'), Trophy, { name: 'achievements' }),
    place('profile', t('profile.title'), User, { name: 'profile' }),
    place('personalize', t('personalize.title'), Brush, { name: 'personalize' }),
    place('settings', t('settings.title'), SettingsCog, { name: 'settings' }),
    place('newInstance', t('rail.newInstance'), Plus, { name: 'newInstance' }),
  ];
}

/** One instance's screens, then the actions a search can safely run. */
function instanceEntries(t: TFunction, instance: Instance, current: boolean, running: boolean): Entry[] {
  const id = instance.id;
  const group = { id: `instance:${id}`, label: instance.name };
  const loader = instance.game.loader !== null;
  const make = (key: string, label: string, icon: IconType, target: Target): Entry => ({
    id: `${group.id}:${key}`,
    group,
    label,
    context: [instance.name],
    current,
    icon,
    target,
  });
  const go = (key: string, label: string, icon: IconType, route: Route) => make(key, label, icon, { kind: 'route', route });
  const act = (action: InstanceAction, label: string, icon: IconType) => make(action, label, icon, { kind: 'action', instance: id, action });
  return [
    go('home', t('quickJump.home'), Play, { name: 'home', id }),
    go('mods', t('home.mods'), TAB_GLYPHS.mod, { name: 'mods', id }),
    go('resourcePacks', t('mods.tabs.resource_pack'), TAB_GLYPHS.resource_pack, { name: 'mods', id, tab: 'resource_pack' }),
    go('shaders', t('mods.tabs.shader'), TAB_GLYPHS.shader, { name: 'mods', id, tab: 'shader' }),
    go('settings', t('home.instanceSettings'), Gear, { name: 'instanceSettings', id }),
    // Mods and shaders run through a loader, so a vanilla instance has nothing to find them for (as Mods & packs says).
    ...(loader ? [go('findMods', t('browse.findMods'), Search, { name: 'browse', kind: 'mod', instance: id })] : []),
    go('findResourcePacks', t('packs.findResourcePacks'), Search, { name: 'browse', kind: 'resource_pack', instance: id }),
    ...(loader ? [go('findShaders', t('packs.findShaders'), Search, { name: 'browse', kind: 'shader', instance: id })] : []),
    act('rename', t('common.rename'), Pencil),
    // A run could save the whole instance over a new picture or a share, so those wait until it stops (as the ⋯ menu).
    ...(running ? [] : [act('image', t('image.menu'), Image)]),
    // A server instance is shared by its invite link instead.
    ...(running || isServer(instance) ? [] : [act('share', t('share.menu'), Share)]),
    act('openFolder', t('common.openFolder'), Folder),
  ];
}

/** A screen's fields, each found on it by the label it shows (`find`; the label itself unless the screen shows another). */
function fields(t: TFunction, group: { id: string; label: string }, route: Route, list: { key: string; label: string; icon: IconType; find?: string }[]): Entry[] {
  return list.map((field) => ({
    id: `${group.id}:${field.key}`,
    group,
    label: field.label,
    context: [group.label],
    current: false,
    icon: field.icon,
    target: { kind: 'field', route, find: field.find ?? field.label },
  }));
}

export function buildEntries({ t, instances, current, running, ownTheme }: Sources): Entry[] {
  const mine = instances.find((i) => i.id === current);
  const ordered = mine ? [mine, ...instances.filter((i) => i !== mine)] : instances;
  const settings = { id: 'settings', label: t('settings.title') };
  const personalize = { id: 'personalize', label: t('personalize.title') };
  return [
    ...places(t),
    ...ordered.flatMap((instance) => instanceEntries(t, instance, instance === mine, running.includes(instance.id))),
    ...fields(t, settings, { name: 'settings' }, [
      { key: 'language', label: t('settings.language'), icon: Languages },
      { key: 'memory', label: t('settings.memory'), icon: MemoryStick },
      { key: 'graphics', label: t('settings.graphics.label'), icon: SettingsCog },
      { key: 'minimize', label: t('settings.minimize'), icon: Minus },
      { key: 'dataFolder', label: t('settings.dataFolder'), icon: Folder },
      { key: 'updates', label: t('settings.updates.title'), icon: Download },
      { key: 'diagnostics', label: t('settings.diagnostics.title'), icon: Terminal },
      { key: 'privacy', label: t('settings.privacy.title'), icon: Shield },
      // The About panel is headed by the launcher's name.
      { key: 'about', label: t('settings.about'), icon: InfoBox, find: t('app.name') },
    ]),
    ...fields(t, personalize, { name: 'personalize' }, [
      { key: 'themes', label: t('personalize.room'), icon: Brush },
      { key: 'customize', label: t('personalize.customize'), icon: Brush },
      // The button colour belongs to a made theme; the player's own theme carries its own, and its two colours instead.
      ownTheme
        ? { key: 'yourColours', label: t('personalize.yourColours'), icon: Brush }
        : { key: 'accent', label: t('personalize.accent'), icon: Brush },
      { key: 'font', label: t('settings.font'), icon: Brush },
      { key: 'iconShape', label: t('settings.iconShape'), icon: Square },
      { key: 'transitions', label: t('settings.transitions'), icon: Brush },
    ]),
  ];
}

/** What Go to shows before anything is typed: the current instance's screens, then Settings and Personalize. */
export function suggested(entries: readonly Entry[]): Entry[] {
  const screens = new Set(['home', 'mods', 'resourcePacks', 'shaders', 'settings']);
  return [
    ...entries.filter((e) => e.current && e.target.kind === 'route' && screens.has(e.id.split(':').pop()!)),
    ...entries.filter((e) => e.id === 'place:personalize' || e.id === 'place:settings'),
  ];
}
