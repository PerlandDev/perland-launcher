import { mockIPC } from '@tauri-apps/api/mocks';
import { expect, it } from 'vitest';
import { commands } from './commands';

it('sends the argument names Rust expects', async () => {
  const calls: [string, unknown][] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return null;
  });
  await commands.play('pack');
  await commands.play('pack', false);
  await commands.joinPreview('l');
  await commands.joinAdd('l');
  await commands.serverStatus('pack');
  await commands.serverRelink('pack', 'l');
  await commands.modSetEnabled('pack', 'a.jar', false);
  await commands.quit(true);
  await commands.catalogLoaders('quilt', '1.20.1');
  await commands.instanceArt('pack');
  await commands.instanceArtSet('pack', { kind: 'generated', seed: 'dunes' });
  expect(calls).toEqual([
    ['play', { id: 'pack', join: true }],
    ['play', { id: 'pack', join: false }],
    ['join_preview', { link: 'l' }],
    ['join_add', { link: 'l' }],
    ['server_status', { id: 'pack' }],
    ['server_relink', { id: 'pack', link: 'l' }],
    ['mod_set_enabled', { id: 'pack', file: 'a.jar', enabled: false }],
    ['quit', { stopGames: true }],
    ['catalog_loaders', { kind: 'quilt', mc: '1.20.1' }],
    ['instance_art', { id: 'pack' }],
    ['instance_art_set', { id: 'pack', choice: { kind: 'generated', seed: 'dunes' } }],
  ]);
});

it('rejects with the LauncherError Rust returned', async () => {
  mockIPC(() => {
    throw { category: 'game', code: 'already_running', message: 'busy', fix: null, detail: null, params: {} };
  });
  await expect(commands.play('pack')).rejects.toMatchObject({ code: 'already_running' });
});
