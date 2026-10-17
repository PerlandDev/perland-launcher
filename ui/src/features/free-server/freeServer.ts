import type { TFunction } from 'i18next';
import { isLauncherError } from '../../ipc/errors';
import type { CreateAllowance, FreeLimits } from '../../ipc/schema';

/** The Engine's rule for a free server's name. */
const NAME_RULE = /^[A-Za-z0-9_-]{1,32}$/;
const FALLBACK_NAME = 'my-server';

export const isServerName = (name: string): boolean => NAME_RULE.test(name);

/** The name the dialog starts with, from the account's display name only (never the email, which may be a kid's and lands on a
 *  public page): accents dropped, spaces to `-`, anything outside the rule removed, cut to 32; `my-server` when nothing is left. */
export function prefillName(displayName: string): string {
  const name = displayName
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .trim()
    .replace(/\s+/g, '-')
    .replace(/[^A-Za-z0-9_-]/g, '')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32);
  return isServerName(name) ? name : FALLBACK_NAME;
}

const gb = (mb: number) => Math.floor(mb / 1024);

/** What every free server gets, in sentences, from the Engine's limits. Without an idle stop a server neither sleeps nor is
 *  deleted for being unused, so those two sentences go. */
export function limitsText(t: TFunction, limits: FreeLimits): string {
  const parts = [
    t('freeServer.limits.players', { count: limits.maxPlayers }),
    t('freeServer.limits.memory', {
      vanilla: gb(limits.memoryMb.vanilla),
      modded: gb(limits.memoryMb.modded),
    }),
  ];
  if (limits.sleepMinutes > 0) parts.push(t('freeServer.limits.sleep'), t('freeServer.limits.unused', { count: limits.unusedDays }));
  return parts.join(' ');
}

/** The `too_many_free_servers` line: at 5 s or less a create is running; otherwise the wait, rounded up to minutes, hours or days. */
export function retryText(t: TFunction, retryAfterSeconds: number): string {
  if (retryAfterSeconds <= 5) return t('freeServer.reason.running');
  const minutes = Math.ceil(retryAfterSeconds / 60);
  const hours = Math.ceil(retryAfterSeconds / 3600);
  const days = Math.ceil(retryAfterSeconds / 86_400);
  const time =
    minutes < 60
      ? t('freeServer.time.minutes', { count: minutes })
      : hours < 24
        ? t('freeServer.time.hours', { count: hours })
        : t('freeServer.time.days', { count: days });
  return t('freeServer.reason.retry', { time });
}

/** Why the player can't create a server now, in one line; null while they may, or when the reason is their own server (its card
 *  shows instead). */
export function reasonLine(t: TFunction, create: CreateAllowance, hasServer: boolean): string | null {
  if (create.allowed) return null;
  switch (create.reason) {
    case 'email_required':
      return t('freeServer.reason.emailRequired');
    case 'free_server_exists':
      return hasServer ? null : t('freeServer.reason.exists');
    case 'too_many_free_servers':
      return create.retryAfter === null ? t('freeServer.reason.other') : retryText(t, create.retryAfter);
    case 'free_servers_full':
      return t('freeServer.reason.full');
    case 'host_full':
      return t('freeServer.reason.hostFull');
    case 'free_servers_paused':
      return t('freeServer.reason.paused');
    case 'free_server_blocked':
      return t('freeServer.reason.blocked');
    default:
      return t('freeServer.reason.other');
  }
}

/** The UI's language as the Engine takes it: `es`, else `en`. */
export const engineLanguage = (language: string): 'en' | 'es' => (language.toLowerCase().startsWith('es') ? 'es' : 'en');

/** Codes after which the create may still have gone through on the Engine: the answer was lost, or the Engine says a server
 *  exists or a create is running. The dialog then waits for the server to show. */
const LOST_CODES = new Set(['offline', 'host_unreachable', 'connection_dropped']);
export function maybeCreated(error: unknown): boolean {
  if (!isLauncherError(error)) return false;
  if (LOST_CODES.has(error.code) || error.code === 'free_server_exists') return true;
  if (error.code !== 'too_many_free_servers') return false;
  const retry = Number(error.params.retryAfter);
  return Number.isFinite(retry) && retry <= 5;
}
