import { describe, expect, it } from 'vitest';
import i18n from '../../i18n';
import { teamRefusalOf } from './teamSteps';

const refused = (code: string, params: Record<string, string> = {}) => ({ category: 'network', code, message: 'x', fix: null, detail: null, params });
const of = (code: string, params?: Record<string, string>) => teamRefusalOf(i18n.t, i18n, refused(code, params));

describe('teamRefusalOf', () => {
  it('keeps the field a plan lock names, as the form names it', () => {
    expect(of('plan_feature_locked', { field: 'ram' })).toEqual({ field: 'ram', text: "Your team's plan doesn't include this.", locked: true });
    expect(of('plan_feature_locked', { field: 'customization.mode' })).toMatchObject({ field: 'settings.gameMode', locked: true });
    // A scope, not a field: the whole create.
    expect(of('plan_feature_locked', { field: 'servers:write' })).toMatchObject({ field: null, locked: true });
  });

  it('words every refusal for a team, never a free server', () => {
    const cases = [
      of('team_server_too_big'),
      of('server_name_taken', { field: 'name' }),
      of('invalid_team_server', { field: 'version' }),
      of('invalid_team_server', { field: 'settings.gameMode' }),
      of('modpack_too_heavy', { requiredMemoryMb: '6144', limitMemoryMb: '4096' }),
      of('modpack_unsupported', { reason: 'loader' }),
      of('modpack_unsupported', { reason: 'not_on_modrinth' }),
      of('share_too_large'),
    ];
    for (const c of cases) expect(c.text).not.toMatch(/free/i);
    expect(cases[0]).toMatchObject({ field: 'ram', text: "Your team's plan has no room for a server this size." });
    expect(cases[1]).toMatchObject({ field: 'name', text: 'Your team already has a server with that name.' });
    expect(cases[4]).toMatchObject({ field: 'ram', text: 'This modpack needs 6 GB of memory. Pick at least that much.' });
  });
});
