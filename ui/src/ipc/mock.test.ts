import { describe, expect, it } from 'vitest';
import { createMockBackend } from './mock';
import type { AccountsFile, InstanceListing, Settings } from './schema';

describe('mock backend', () => {
  it('serves the contract fixtures', async () => {
    const { handle } = createMockBackend();
    const listing = (await handle('instances_list')) as InstanceListing;
    expect(listing.instances.map((i) => i.id)).toEqual(['asalith-fields', 'creative-flat']);
    expect(listing.damaged).toHaveLength(1);
    const settings = (await handle('settings_get')) as Settings;
    expect(settings.firstRunDone).toBe(true);
  });

  it('starts empty for first run', async () => {
    const { handle } = createMockBackend({ firstRun: true });
    expect(((await handle('instances_list')) as InstanceListing).instances).toEqual([]);
    expect(((await handle('accounts_list')) as AccountsFile).accounts).toEqual([]);
    expect(((await handle('settings_get')) as Settings).firstRunDone).toBe(false);
  });

  it('creates instances and toggles mods like the real backend', async () => {
    const { handle } = createMockBackend();
    const created = (await handle('instance_create', {
      request: { name: 'New One', minecraft: '1.21.1', loader: null },
    })) as { id: string };
    expect(created.id).toBe('new-one');
    const mods = (await handle('mod_set_enabled', {
      id: 'asalith-fields',
      file: 'zoomify-2.11.2.jar.disabled',
      enabled: true,
    })) as { fileName: string; enabled: boolean }[];
    expect(mods.find((m) => m.fileName === 'zoomify-2.11.2.jar')?.enabled).toBe(true);
  });

  it('rejects offline accounts in a release-like build', async () => {
    const { handle } = createMockBackend({ release: true });
    await expect(handle('account_add_offline', { name: 'Steve' })).rejects.toMatchObject({ code: 'offline_locked' });
  });
});
