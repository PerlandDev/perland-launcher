import type { i18n as I18n } from 'i18next';
import type { LauncherError, TaskId } from './schema';

export interface ErrorText {
  message: string;
  fix: string | null;
  detail: string | null;
}

export function isLauncherError(value: unknown): value is LauncherError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    'message' in value &&
    'category' in value &&
    'params' in value
  );
}

/** A failure in the player's language: translated by code, or the core's English as a fallback. */
export function describeError(i18n: I18n, error: unknown): ErrorText {
  if (!isLauncherError(error)) {
    return {
      message: i18n.t('errors.internal.message'),
      fix: i18n.t('errors.internal.fix'),
      detail: error instanceof Error ? error.message : String(error),
    };
  }
  const key = `errors.${error.code}`;
  const known = i18n.exists(`${key}.message`);
  const message = known ? i18n.t(`${key}.message`, error.params) : error.message;
  // A translated message never pairs with an untranslated fix: no mixed-language cards.
  const fix = i18n.exists(`${key}.fix`) ? i18n.t(`${key}.fix`, error.params) : known ? null : error.fix;
  return { message, fix, detail: error.detail };
}

export function taskLabel(i18n: I18n, task: TaskId): string {
  const key = `progress.${task.key}`;
  return i18n.exists(key) ? i18n.t(key, task.params) : task.key;
}

/** Text for "Copy for support". The core already redacted tokens and the home folder. */
export function supportText(lines: string[]): string {
  return lines.join('\n');
}
