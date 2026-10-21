import type { TFunction } from 'i18next';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { FreeLimits, FreeServerSettings } from '../../ipc/schema';

// Options for a new free server (spec 2026-10-21 §4.1): a disclosure under the name, closed by default, that starts on what
// the server gets anyway. Only what the player changed is sent, so an untouched form makes exactly today's server.

export const GAME_MODES = ['survival', 'creative', 'adventure', 'hardcore'] as const;
export const DIFFICULTIES = ['peaceful', 'easy', 'normal', 'hard'] as const;
export type GameMode = (typeof GAME_MODES)[number];
export type Difficulty = (typeof DIFFICULTIES)[number];

export interface OptionsState {
  gameMode: GameMode;
  /** The player's pick, kept while Hardcore shows Hard, so switching back gives it back. */
  difficulty: Difficulty;
  /** As typed. */
  maxPlayers: string;
  pvp: boolean;
  whitelist: boolean;
  seed: string;
  motd: string;
  /** The Engine's player limit: the default, and the most the field takes. */
  limit: number;
}

/** What a server left alone gets (the Engine's defaults): survival, easy, the player limit, PvP on, whitelist off. */
const DEFAULTS = { gameMode: 'survival', difficulty: 'easy', pvp: true, whitelist: false } as const;

export const initialOptions = (limits: FreeLimits): OptionsState => ({
  ...DEFAULTS,
  maxPlayers: String(limits.maxPlayers),
  seed: '',
  motd: '',
  limit: limits.maxPlayers,
});

/** The settings to send: only the fields changed from the defaults, or null when nothing changed. Hardcore sends no
 *  difficulty (the Engine makes it Hard). Call it after `problems` found none. */
export function settingsOf(state: OptionsState): FreeServerSettings | null {
  const settings: FreeServerSettings = {};
  if (state.gameMode !== DEFAULTS.gameMode) settings.gameMode = state.gameMode;
  if (state.gameMode !== 'hardcore' && state.difficulty !== DEFAULTS.difficulty) settings.difficulty = state.difficulty;
  const max = Number(state.maxPlayers);
  if (state.maxPlayers.trim() !== '' && max !== state.limit) settings.maxPlayers = max;
  if (state.pvp !== DEFAULTS.pvp) settings.pvp = state.pvp;
  if (state.whitelist !== DEFAULTS.whitelist) settings.whitelist = state.whitelist;
  if (state.seed.trim() !== '') settings.seed = state.seed.trim();
  if (state.motd.trim() !== '') settings.motd = state.motd.trim();
  return Object.keys(settings).length > 0 ? settings : null;
}

const bytes = (text: string) => new TextEncoder().encode(text).length;

/** What the Engine would refuse, by its field name (`settings.maxPlayers`, ...), in words. */
function problems(t: TFunction, state: OptionsState, limits: FreeLimits): Record<string, string> {
  const found: Record<string, string> = {};
  const max = Number(state.maxPlayers);
  if (state.maxPlayers.trim() === '' || !Number.isInteger(max) || max < 1 || max > state.limit) {
    found['settings.maxPlayers'] = t('freeServer.options.maxPlayersInvalid', { max: state.limit });
  }
  if (bytes(state.seed.trim()) > (limits.settings?.seedMaxBytes ?? 64)) found['settings.seed'] = t('freeServer.options.seedInvalid');
  const motd = state.motd.trim();
  if (bytes(motd) > (limits.settings?.motdMaxBytes ?? 64) || motd.includes('§')) found['settings.motd'] = t('freeServer.options.motdInvalid');
  return found;
}

/** The words for an option the Engine refused, by its `field`. */
export function optionRefusal(t: TFunction, field: string, limit: number): string {
  switch (field) {
    case 'settings.maxPlayers':
      return t('freeServer.options.maxPlayersInvalid', { max: limit });
    case 'settings.seed':
      return t('freeServer.options.seedInvalid');
    case 'settings.motd':
      return t('freeServer.options.motdInvalid');
    default:
      return t('freeServer.options.refused');
  }
}

// Typed text stays on one line: a newline or tab becomes a space (the Engine refuses control characters).
export const oneLine = (text: string) => text.replace(/[\r\n\t]/g, ' ');

/** The Options state of one dialog. `show` is false on an Engine that doesn't take settings: nothing shows, nothing is sent. */
export function useServerOptions(limits: FreeLimits) {
  const { t } = useTranslation();
  const show = limits.settings !== null;
  const [state, setState] = useState(() => initialOptions(limits));
  const [open, setOpen] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});
  return {
    show,
    limits,
    state,
    open,
    setOpen,
    errors,
    set: (patch: Partial<OptionsState>) => {
      setState((s) => ({ ...s, ...patch }));
      setErrors((e) => {
        const next = { ...e };
        for (const key of Object.keys(patch)) delete next[`settings.${key}`];
        if ('gameMode' in patch) delete next['settings.difficulty'];
        return next;
      });
    },
    /** Whether the options may be sent; a problem opens the disclosure and shows under its field. */
    check: (): boolean => {
      if (!show) return true;
      const found = problems(t, state, limits);
      setErrors(found);
      if (Object.keys(found).length === 0) return true;
      setOpen(true);
      return false;
    },
    /** A refusal of one option (`settings.<name>`), shown under it. */
    refuse: (field: string, text: string) => {
      setErrors({ [field]: text });
      setOpen(true);
    },
    clearErrors: () => setErrors({}),
    settings: (): FreeServerSettings | null => (show ? settingsOf(state) : null),
  };
}

export type ServerOptionsModel = ReturnType<typeof useServerOptions>;

export const modeLabel = (t: TFunction, mode: GameMode) => (mode === 'hardcore' ? t('freeServer.options.hardcore') : t(`manage.settings.modes.${mode}`));
