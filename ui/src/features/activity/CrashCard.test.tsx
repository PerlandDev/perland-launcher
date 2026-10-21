import { act, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import listingFx from '../../ipc/contract/instance_listing.json';
import { exitOf } from '../../ipc/mock';
import { mockCrash, type CrashScenario } from '../../ipc/mockCrash';
import type { CrashFix, ExitInfo, Instance, InstanceListing } from '../../ipc/schema';
import { routeSnapshot, resetRoute } from '../../state/route';
import { runStore } from '../../state/runStore';
import { toastsSnapshot, clearToasts } from '../../state/toasts';
import { renderApp, sent } from '../../test/backend';
import { CrashCard } from './CrashCard';
import { DRIVER_PAGES } from './driverPages';

const instance = (JSON.parse(JSON.stringify(listingFx)) as InstanceListing).instances.find((i) => i.id === 'asalith-fields')!;

function crashed(scenario: CrashScenario, extra: Partial<ExitInfo> = {}): ExitInfo {
  const crash = mockCrash(scenario);
  return exitOf({ code: 1, cause: crash.cause, fixes: crash.fixes, ...extra });
}

function show(exit: ExitInfo, of: Instance = instance) {
  act(() => {
    runStore.apply({ type: 'run_state', instance: of.id, state: 'queued', exit: null, update: null });
    runStore.apply({ type: 'run_state', instance: of.id, state: 'idle', exit, update: null });
  });
  return renderApp(<CrashCard instance={of} exit={exit} lines={['[12:00:01] line']} />);
}

const buttons = () => screen.getAllByRole('button').map((b) => b.textContent);

describe('CrashCard', () => {
  beforeEach(() => {
    resetRoute();
    clearToasts();
    runStore.reset();
  });

  it('says the cause in plain words with its next step', async () => {
    show(crashed('clash'));
    expect(await screen.findByRole('heading', { name: "Sodium and Fabric API don't work together." })).toBeInTheDocument();
    expect(screen.getByText('Turn one of them off and press Play again.')).toBeInTheDocument();
  });

  it('keeps the generic card for an unknown cause', async () => {
    show(exitOf({ code: 1 }));
    expect(await screen.findByRole('heading', { name: 'Asalith Fields closed unexpectedly' })).toBeInTheDocument();
    expect(screen.getByText('Exit code 1')).toBeInTheDocument();
    const copy = screen.getByRole('button', { name: 'Copy for support' });
    expect(copy).not.toHaveClass('btn-ghost');
    expect(screen.queryByRole('button', { name: 'Details' })).not.toBeInTheDocument();
  });

  it('raises the first fix and keeps the rest ghost, never lit', async () => {
    show(crashed('clash'));
    const first = await screen.findByRole('button', { name: 'Turn off Fabric API' });
    const second = screen.getByRole('button', { name: 'Turn off Sodium' });
    expect(first).not.toHaveClass('btn-ghost');
    expect(second).toHaveClass('btn-ghost');
    expect(screen.getByRole('button', { name: 'Copy for support' })).toHaveClass('btn-ghost');
    for (const button of screen.getAllByRole('button')) expect(button).not.toHaveClass('btn-primary');
    expect(buttons().indexOf('Turn off Fabric API')).toBeLessThan(buttons().indexOf('Turn off Sodium'));
  });

  it('turns the mod off, says so, and dismisses the card', async () => {
    const { user, calls } = show(crashed('mixin'));
    await user.click(await screen.findByRole('button', { name: 'Turn off Sodium' }));
    await waitFor(() => expect(sent(calls, 'mod_set_enabled')).toEqual([{ id: 'asalith-fields', file: 'sodium-fabric-0.5.8+mc1.20.1.jar', enabled: false }]));
    await waitFor(() => expect(runStore.run('asalith-fields').lastExit).toBeNull());
    expect(toastsSnapshot().map((t) => t.notice?.title)).toContain('Sodium is off. Press Play to try again.');
  });

  it('shows Find only for a mod Modrinth has', async () => {
    const { user } = show(crashed('missing'));
    await user.click(await screen.findByRole('button', { name: 'Find Fabric API' }));
    expect(screen.getByRole('heading', { name: "Iris needs Fabric API, which isn't installed." })).toBeInTheDocument();
    expect(routeSnapshot()).toEqual({ name: 'browse', kind: 'mod', instance: 'asalith-fields', project: 'fabric-api' });
  });

  it('hides Find for a mod Modrinth does not have, and the next fix is the primary', async () => {
    const fixes: CrashFix[] = [{ kind: 'find_mod', project: 'not-on-modrinth' }, { kind: 'java_settings' }];
    show(crashed('missing', { fixes }));
    const settings = await screen.findByRole('button', { name: 'Java settings' });
    expect(settings).not.toHaveClass('btn-ghost');
    expect(screen.queryByRole('button', { name: /^Find/ })).not.toBeInTheDocument();
  });

  it('raises Copy for support when no fix can show', async () => {
    show(crashed('missing', { fixes: [{ kind: 'find_mod', project: 'not-on-modrinth' }] }));
    expect(await screen.findByRole('heading', { name: "Iris needs fabric-api, which isn't installed." })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Copy for support' })).not.toHaveClass('btn-ghost'));
    expect(screen.queryByRole('button', { name: /^Find/ })).not.toBeInTheDocument();
  });

  it('raises memory within what the computer has', async () => {
    const { user, calls } = show(crashed('memory'));
    await user.click(await screen.findByRole('button', { name: 'Raise to 6 GB' }));
    const update = sent(calls, 'instance_update')[0] as { instance: { java: { memoryMb: number } } };
    expect(update.instance.java.memoryMb).toBe(6144);
  });

  it('opens the settings when there is no memory left to give', async () => {
    const full = { ...instance, java: { ...instance.java, memoryMb: 65536 } };
    const { user } = show(crashed('memory'), full);
    await user.click(await screen.findByRole('button', { name: 'Memory settings' }));
    expect(routeSnapshot()).toEqual({ name: 'instanceSettings', id: 'asalith-fields' });
  });

  it('switches back to the recommended Java', async () => {
    const custom = { ...instance, java: { ...instance.java, path: '/java/bin/java' } };
    const { user, calls } = show(crashed('java', { fixes: [{ kind: 'recommended_java' }] }), custom);
    await user.click(await screen.findByRole('button', { name: 'Use the recommended Java' }));
    const update = sent(calls, 'instance_update')[0] as { instance: { java: { path: string | null } } };
    expect(update.instance.java.path).toBeNull();
  });

  it('repairs and plays', async () => {
    const { user, calls } = show(crashed('files'));
    await user.click(await screen.findByRole('button', { name: 'Repair and play' }));
    expect(sent(calls, 'play_repair')).toEqual([{ id: 'asalith-fields' }]);
  });

  it('opens the vendor driver page', async () => {
    const { user, calls } = show(crashed('driver'));
    await user.click(await screen.findByRole('button', { name: 'Update drivers' }));
    expect(sent(calls, 'open_link')).toEqual([{ url: DRIVER_PAGES.nvidia }]);
  });

  it('shows the matched log line behind Details', async () => {
    const { user } = show(crashed('mixin'));
    await user.click(await screen.findByRole('button', { name: 'Details' }));
    expect(screen.getByText(/Mixin apply for mod sodium failed/)).toBeInTheDocument();
  });

  it('reads in Spanish', async () => {
    const { default: i18n } = await import('../../i18n');
    await act(() => i18n.changeLanguage('es'));
    try {
      show(crashed('clash'));
      expect(await screen.findByRole('heading', { name: 'Sodium y Fabric API no funcionan juntos.' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Desactivar Fabric API' })).toBeInTheDocument();
    } finally {
      await act(() => i18n.changeLanguage('en'));
    }
  });
});
