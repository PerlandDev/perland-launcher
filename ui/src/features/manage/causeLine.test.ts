import { describe, expect, it } from 'vitest';
import i18n from '../../i18n';
import { causeLine } from './causeLine';

const t = i18n.t.bind(i18n);
const cause = (code: string, subject: string | null = null) => ({ code, known: true, subject, params: {} });

describe('causeLine', () => {
  it('words a known cause', () => {
    expect(causeLine(t, cause('out_of_memory'))).toBe('The server ran out of memory and stopped.');
  });

  it('names the mod or plugin it is about', () => {
    expect(causeLine(t, cause('mod_missing_dependency', 'Create'))).toBe("Create needs another mod that isn't installed.");
    expect(causeLine(t, cause('plugin_load_failed', 'EssentialsX'))).toBe("EssentialsX couldn't load.");
  });

  it('says "a mod" when the diagnosis names none', () => {
    expect(causeLine(t, cause('mod_missing_dependency'))).toBe("A mod needs another mod that isn't installed.");
  });

  it('falls back to a plain line for an unknown code, with the subject when there is one', () => {
    expect(causeLine(t, cause('unclassified_exception'))).toBe('The server stopped because of an error.');
    expect(causeLine(t, cause('something_new', 'Sodium'))).toBe('The server stopped because of an error in Sodium.');
  });

  it('every known code has words in both languages', () => {
    for (const code of [
      'out_of_memory',
      'mod_missing_dependency',
      'mod_incompatible',
      'client_only_mod',
      'mixin_apply_failed',
      'plugin_load_failed',
      'plugin_missing_dependency',
      'java_version_mismatch',
      'version_not_available',
      'source_download_failed',
      'config_load_failed',
      'port_bind_failed',
      'installer_failed',
    ]) {
      expect(i18n.exists(`manage.cause.${code}`, { lng: 'en' }), code).toBe(true);
      expect(i18n.exists(`manage.cause.${code}`, { lng: 'es' }), code).toBe(true);
    }
  });
});
