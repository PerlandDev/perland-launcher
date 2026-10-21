import type { TFunction } from 'i18next';
import type { ManageCause } from '../../ipc/schema';

/** The diagnosis codes the launcher words itself; any other reads as a plain "stopped because of an error". */
const MOD_CODES = ['mod_missing_dependency', 'mod_incompatible', 'client_only_mod', 'mixin_apply_failed'];
const PLUGIN_CODES = ['plugin_load_failed', 'plugin_missing_dependency'];
export const KNOWN_CAUSES: readonly string[] = [
  'out_of_memory',
  ...MOD_CODES,
  ...PLUGIN_CODES,
  'java_version_mismatch',
  'version_not_available',
  'source_download_failed',
  'config_load_failed',
  'port_bind_failed',
  'installer_failed',
];

/** Why the server's last run failed, in one plain sentence. `subject` (the mod or plugin) is named when the diagnosis
 *  has one; a mod or plugin sentence without one says "A mod" / "A plugin". */
export function causeLine(t: TFunction, cause: ManageCause): string {
  const subjectPart = cause.subject ? t('manage.cause.subjectPart', { subject: cause.subject }) : '';
  if (!KNOWN_CAUSES.includes(cause.code)) return t('manage.cause.unknown', { subjectPart });
  const fallback = PLUGIN_CODES.includes(cause.code) ? t('manage.cause.aPlugin') : t('manage.cause.aMod');
  return t(`manage.cause.${cause.code}`, { subject: cause.subject ?? fallback, subjectPart });
}
