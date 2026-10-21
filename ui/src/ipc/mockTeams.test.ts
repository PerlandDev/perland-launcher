import { describe, expect, it } from 'vitest';
import { createMockBackend } from './mock';
import { MOCK_TEAM_TENANT } from './mockManage';
import { MOCK_FULL_TENANT } from './mockTeams';
import type { ManagedServer, TeamServerCreated } from './schema';

const request = (tenantId: string, name = 'Amigos') => ({
  request: { tenantId, name, type: 'vanilla', version: '1.21.5', loaderVersion: null, memoryGb: 4, settings: null },
});

describe('mock teams', () => {
  it('lists teams only while signed in with teams on', async () => {
    expect(await createMockBackend({ perland: 'signedIn', teams: 'some' }).handle('creatable_teams')).toHaveLength(3);
    expect(await createMockBackend({ perland: 'signedIn' }).handle('creatable_teams')).toEqual([]);
    expect(await createMockBackend({ teams: 'some' }).handle('creatable_teams')).toEqual([]);
  });

  it('makes a server Manage lists, stopped on a team with no room', async () => {
    const b = createMockBackend({ perland: 'signedIn', teams: 'some' });
    const made = (await b.handle('team_create', request(MOCK_FULL_TENANT))) as TeamServerCreated;
    expect(made).toMatchObject({ started: false, notStartedReason: 'quota_exceeded', stack: 'Amigos' });
    const managed = (await b.handle('managed_servers')) as ManagedServer[];
    expect(managed.find((s) => s.tenantId === MOCK_FULL_TENANT)).toMatchObject({ stack: 'Amigos', state: 'stopped' });
    await expect(b.handle('team_create', request(MOCK_FULL_TENANT))).rejects.toMatchObject({ code: 'server_name_taken' });
    await expect(b.handle('team_create', { request: { ...request(MOCK_TEAM_TENANT).request, memoryGb: 9 } })).rejects.toMatchObject({
      code: 'invalid_team_server',
      params: { field: 'ram' },
    });
  });
});
