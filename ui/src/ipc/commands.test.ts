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
  await commands.modSetEnabled('pack', 'a.jar', false);
  await commands.quit(true);
  await commands.catalogLoaders('quilt', '1.20.1');
  expect(calls).toEqual([
    ['play', { id: 'pack' }],
    ['mod_set_enabled', { id: 'pack', file: 'a.jar', enabled: false }],
    ['quit', { stopGames: true }],
    ['catalog_loaders', { kind: 'quilt', mc: '1.20.1' }],
  ]);
});

it('rejects with the LauncherError Rust returned', async () => {
  mockIPC(() => {
    throw { category: 'game', code: 'already_running', message: 'busy', fix: null, detail: null, params: {} };
  });
  await expect(commands.play('pack')).rejects.toMatchObject({ code: 'already_running' });
});
