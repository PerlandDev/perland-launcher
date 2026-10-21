import type { i18n as I18n, TFunction } from 'i18next';
import type { ReactNode } from 'react';
import { describeError, isLauncherError } from '../../ipc/errors';
import type { TeamServerCreated } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { CopyRow } from '../../ui/CopyRow';
import { isOptionField, optionRefusal, TEAM_OPTION_LIMITS } from './optionRules';

// A server made on a team (spec 2026-10-21 §6.3): its refusals in a team's words, never a free server's, and its last step,
// which leads to Manage (a team server's invite is the team's choice, so there is no invite step).

/** Where a team refusal goes: under a field of the form (`name`, `type`, `version`, `loaderVersion`, `ram`, `settings.<name>`),
 *  or a line under the form (null). `locked` is a field (or the whole create, with `field` null) the team's plan doesn't
 *  include: it shows locked with Upgrade. */
export interface TeamRefusal {
  field: string | null;
  text: string;
  locked: boolean;
}

/** The form's own field for an Engine field name (`ram`, `memory`, `customization.maxPlayers`, `mode`, ...). */
function formField(field: string | undefined): string | null {
  if (!field) return null;
  if (['name', 'serverName'].includes(field)) return 'name';
  if (['ram', 'memory', 'cpu'].includes(field)) return 'ram';
  if (['type', 'version', 'loaderVersion'].includes(field)) return field;
  const setting = field.replace(/^(settings|customization)\./, '');
  const known: Record<string, string> = { mode: 'gameMode', hardcore: 'gameMode', enableWhitelist: 'whitelist', enforceWhitelist: 'whitelist' };
  const option = `settings.${known[setting] ?? setting}`;
  return isOptionField(option) ? option : null;
}

export function teamRefusalOf(t: TFunction, i18n: I18n, error: unknown): TeamRefusal {
  const line = (text: string, field: string | null = null): TeamRefusal => ({ field, text, locked: false });
  if (!isLauncherError(error)) return line(describeError(i18n, error).message);
  const field = formField(error.params.field);
  switch (error.code) {
    case 'invalid_team_server':
      if (field === 'name') return line(t('freeServer.pick.nameInvalid'), field);
      if (field === 'version') return line(t('teamServer.versionRefused'), field);
      if (field === 'ram') return line(t('teamServer.tooBig'), field);
      if (field?.startsWith('settings.')) return line(optionRefusal(t, field, TEAM_OPTION_LIMITS.maxPlayers, 'teamServer.optionRefused'), field);
      if (error.params.field === 'pack') return line(t('teamServer.packRefused'));
      return line(t('teamServer.refused'), field);
    case 'server_name_taken':
      return line(t('teamServer.nameTaken'), 'name');
    case 'team_server_too_big':
      return line(t('teamServer.tooBig'), 'ram');
    case 'plan_feature_locked':
      return { field, text: t('teamServer.locked'), locked: true };
    case 'modpack_too_heavy': {
      const need = Math.ceil(Number(error.params.requiredMemoryMb) / 1024);
      return Number.isFinite(need) ? line(t('teamServer.tooHeavy', { need }), 'ram') : line(t('teamServer.packRefused'));
    }
    case 'modpack_unsupported': {
      const key = `teamServer.unsupported.${error.params.reason ?? ''}`;
      return line(i18n.exists(key) ? t(key) : t('teamServer.packRefused'));
    }
    case 'share_too_large':
      return line(t('teamServer.packTooBig'));
    default:
      return line(describeError(i18n, error).message);
  }
}

/** The last step of a team create: the server is ready, or made without room to run it (`started: false`), its address to
 *  copy when it has one, and Manage, the step's one lit key. */
export function teamReadyStep(
  t: TFunction,
  server: TeamServerCreated,
  onManage: () => void,
  onClose: () => void,
): { title: string; actions: ReactNode; content: ReactNode } {
  const line = server.started
    ? t('teamServer.ready.hint')
    : server.notStartedReason === 'host_full'
      ? t('teamServer.ready.hostFull')
      : t('teamServer.ready.noRoom');
  return {
    title: t('teamServer.ready.title'),
    actions: (
      <>
        <Button onClick={onClose}>{t('common.close')}</Button>
        <Button variant="primary" onClick={onManage}>
          {t('teamServer.ready.manage')}
        </Button>
      </>
    ),
    content: (
      <>
        <p className="text-sm" role={server.started ? undefined : 'status'}>
          {line}
        </p>
        {server.address && (
          <CopyRow
            text={server.address}
            label={t('freeServer.ready.address')}
            copy={t('freeServer.ready.copyAddress')}
            copied={t('freeServer.ready.addressCopied')}
          />
        )}
      </>
    ),
  };
}
