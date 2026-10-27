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
  await commands.play('pack', true);
  await commands.joinPreview('l');
  await commands.joinAdd('l');
  await commands.serverStatus('pack');
  await commands.serverRelink('pack', 'l');
  await commands.serverUnlink('pack');
  const query = { sort: 'players', modded: null, minecraft: null, hasRoom: false, category: null, language: null, region: null, offset: 0 } as const;
  await commands.serversList(query);
  await commands.serversIcon('t', 'v1');
  await commands.modSetEnabled('pack', 'a.jar', false);
  await commands.quit(true);
  await commands.catalogLoaders('quilt', '1.20.1');
  await commands.instanceArt('pack');
  await commands.instanceArtSet('pack', { kind: 'generated', seed: 'dunes' });
  await commands.perlandSignInDiscord('es');
  await commands.accountSignInMicrosoft('es');
  await commands.perlandEmailStart('a@b.c', 'en');
  await commands.perlandEmailVerify('a@b.c', '123456');
  await commands.yourServersFollow('inv');
  await commands.yourServersUnfollow('inv');
  await commands.yourServersJoined('inv');
  await commands.perlandLinkCode();
  await commands.perlandLinkPoll();
  await commands.perlandUnlink();
  await commands.perlandStats();
  await commands.restorePoint('pack');
  await commands.restoreGoBack('pack');
  await commands.packUpdateAgain('pack');
  expect(calls).toEqual([
    ['play', { id: 'pack', join: false }],
    ['play', { id: 'pack', join: true }],
    ['join_preview', { link: 'l' }],
    ['join_add', { link: 'l' }],
    ['server_status', { id: 'pack' }],
    ['server_relink', { id: 'pack', link: 'l' }],
    ['server_unlink', { id: 'pack' }],
    ['servers_list', { query }],
    ['servers_icon', { token: 't', iconVersion: 'v1' }],
    ['mod_set_enabled', { id: 'pack', file: 'a.jar', enabled: false }],
    ['quit', { stopGames: true }],
    ['catalog_loaders', { kind: 'quilt', mc: '1.20.1' }],
    ['instance_art', { id: 'pack' }],
    ['instance_art_set', { id: 'pack', choice: { kind: 'generated', seed: 'dunes' } }],
    ['perland_sign_in_discord', { language: 'es' }],
    ['account_sign_in_microsoft', { language: 'es' }],
    ['perland_email_start', { email: 'a@b.c', language: 'en' }],
    ['perland_email_verify', { email: 'a@b.c', code: '123456' }],
    ['your_servers_follow', { invite: 'inv' }],
    ['your_servers_unfollow', { invite: 'inv' }],
    ['your_servers_joined', { invite: 'inv' }],
    ['perland_link_code', {}],
    ['perland_link_poll', {}],
    ['perland_unlink', {}],
    ['perland_stats', {}],
    ['restore_point', { id: 'pack' }],
    ['restore_go_back', { id: 'pack' }],
    ['pack_update_again', { id: 'pack' }],
  ]);
});

it('sends Manage arguments as Rust names them', async () => {
  const calls: [string, unknown][] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return null;
  });
  const change = { list: 'bans' as const, add: true, names: ['Griefer'] };
  const save = {
    revision: 'r',
    motd: null,
    mode: 'survival',
    difficulty: 'easy',
    maxPlayers: 10,
    pvp: true,
    enableWhitelist: false,
    version: null,
    apply: 'now',
  } as const;
  await commands.perlandSignInGithub('es');
  await commands.managedServers();
  await commands.manageDetail('t', 'w');
  await commands.manageAction('t', 'w', 'restart');
  await commands.manageDiagnosis('t', 'w');
  await commands.managePlayers('t', 'w');
  await commands.managePlayersChange('t', 'w', change);
  await commands.manageSettings('t', 'w');
  await commands.manageSettingsSave('t', 'w', save);
  await commands.manageConsole('t', 'w', 'list', false);
  await commands.manageConsoleOpen('t', 'w');
  await commands.manageConsoleClose('t', 'w');
  await commands.manageOpenPanel('t', 'w');
  await commands.manageOpenPanel('t', null);
  expect(calls).toEqual([
    ['perland_sign_in_github', { language: 'es' }],
    ['managed_servers', {}],
    ['manage_detail', { tenantId: 't', stack: 'w' }],
    ['manage_action', { tenantId: 't', stack: 'w', action: 'restart' }],
    ['manage_diagnosis', { tenantId: 't', stack: 'w' }],
    ['manage_players', { tenantId: 't', stack: 'w' }],
    ['manage_players_change', { tenantId: 't', stack: 'w', change }],
    ['manage_settings', { tenantId: 't', stack: 'w' }],
    ['manage_settings_save', { tenantId: 't', stack: 'w', save }],
    ['manage_console', { tenantId: 't', stack: 'w', command: 'list', confirm: false }],
    ['manage_console_open', { tenantId: 't', stack: 'w' }],
    ['manage_console_close', { tenantId: 't', stack: 'w' }],
    ['manage_open_panel', { tenantId: 't', stack: 'w' }],
    ['manage_open_panel', { tenantId: 't', stack: null }],
  ]);
});

it('rejects with the LauncherError Rust returned', async () => {
  mockIPC(() => {
    throw { category: 'game', code: 'already_running', message: 'busy', fix: null, detail: null, params: {} };
  });
  await expect(commands.play('pack')).rejects.toMatchObject({ code: 'already_running' });
});
