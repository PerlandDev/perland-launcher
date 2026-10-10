import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { describe, expect, it, vi } from 'vitest';
import { createMockBackend, stopMockRuns } from './mock';
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

  it('returns fresh objects like real IPC, never live state', async () => {
    const { handle } = createMockBackend();
    const first = (await handle('instances_list')) as InstanceListing;
    const second = (await handle('instances_list')) as InstanceListing;
    expect(second).toEqual(first);
    expect(second).not.toBe(first);

    const before = (await handle('accounts_list')) as AccountsFile;
    const activeBefore = before.active;
    const other = before.accounts.find((a) => a.uuid !== activeBefore);
    const after = (await handle('account_set_active', { uuid: other?.uuid })) as AccountsFile;
    expect(after.active).toBe(other?.uuid);
    expect(before.active).toBe(activeBefore);
    expect(((await handle('accounts_list')) as AccountsFile).active).toBe(other?.uuid);

    const renamed = (await handle('instance_rename', { id: 'asalith-fields', name: 'Renamed' })) as { name: string };
    expect(renamed.name).toBe('Renamed');
    expect(first.instances.find((i) => i.id === 'asalith-fields')?.name).not.toBe('Renamed');
  });

  it('rejects offline accounts in a release-like build', async () => {
    const { handle } = createMockBackend({ release: true });
    await expect(handle('account_add_offline', { name: 'Steve' })).rejects.toMatchObject({ code: 'offline_locked' });
  });
});

describe('scripted runs', () => {
  it('stop for good when a test tears the backend down', async () => {
    vi.useFakeTimers();
    try {
      const emitted: unknown[] = [];
      mockIPC((cmd, args) => {
        if (cmd === 'plugin:event|emit') emitted.push(args);
        return null;
      });
      const { handle } = createMockBackend();
      await handle('play', { id: 'asalith-fields' });
      await vi.advanceTimersByTimeAsync(5_000); // queued → preparing → running
      expect(emitted.length).toBeGreaterThan(0);
      stopMockRuns();
      emitted.length = 0;
      await vi.advanceTimersByTimeAsync(5_000);
      expect(emitted).toEqual([]);
    } finally {
      clearMocks();
      vi.useRealTimers();
    }
  });
});
