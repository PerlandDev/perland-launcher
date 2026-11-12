import { act, screen, waitFor, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import type { MedalsView } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { Profile } from './Profile';

const setUp = () => screen.queryByRole('region', { name: 'Finish setting up' });
const steps = () => within(setUp()!).getAllByRole('listitem');

it('heads the page with who is playing, their kind of account and the time played on this computer', async () => {
  renderApp(<Profile />, { playtime: { 'asalith-fields': 12 * 3600, 'creative-flat': 30 * 60 } });
  expect(await screen.findByRole('heading', { name: 'Steve', level: 1 })).toBeInTheDocument();
  expect(await screen.findByText('Offline account · Time played on this computer: 12 h 30 min')).toBeInTheDocument();
});

it('leaves the time out before a minute is played', async () => {
  const { calls } = renderApp(<Profile />, { playtime: { 'asalith-fields': 59 } });
  expect(await screen.findByRole('heading', { name: 'Steve', level: 1 })).toBeInTheDocument();
  await waitFor(() => expect(sent(calls, 'playtime_list')).toHaveLength(1));
  await act(async () => {});
  expect(screen.queryByText(/Time played on this computer/)).toBeNull();
  expect(screen.getByText('Offline account', { selector: 'header p' })).toBeInTheDocument();
});

it('is Your profile while there is no account, and asks for one', async () => {
  const { backend } = renderApp(<Profile />);
  backend.state.accounts.accounts = [];
  backend.state.accounts.active = null;
  expect(await screen.findByRole('heading', { name: 'Your profile', level: 1 })).toBeInTheDocument();
  expect(await screen.findByText('No accounts yet.')).toBeInTheDocument();
  // Nobody is playing yet, so the checklist has no done line: only what to connect.
  expect(await screen.findByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
  expect(screen.queryByText(/Playing as/)).toBeNull();
});

it('holds the skin, the accounts and a look at the medals on one page', async () => {
  renderApp(<Profile />, { perland: 'linked', microsoft: 'signedIn' });
  for (const name of ['Your skin', 'Accounts', 'Medals']) expect(await screen.findByRole('heading', { name, level: 2 })).toBeInTheDocument();
  // The medals themselves are on their own screen: here there is only the look at them.
  expect(screen.queryByRole('heading', { name: 'Medals earned' })).toBeNull();
  expect(screen.queryByRole('heading', { name: /Minecraft achievements/ })).toBeNull();
});

it('shows how many medals are earned, the latest ones and what is next, and opens Medals and achievements', async () => {
  const { user } = renderApp(<Profile />, { playtime: { 'asalith-fields': 12 * 3600 } });
  const medals = (await screen.findByRole('heading', { name: 'Medals', level: 2 })).closest('section')!;
  expect(await within(medals).findByText(/^\d+ of 24 medals$/)).toBeInTheDocument();
  // The latest: at most three, each with its state and the day it was earned.
  const latest = within(await within(medals).findByRole('list', { name: 'Latest' })).getAllByRole('listitem');
  expect(latest.length).toBeGreaterThan(0);
  expect(latest.length).toBeLessThanOrEqual(3);
  const time = latest.find((li) => li.textContent?.includes('Time played'))!;
  expect(time).toHaveTextContent('Iron · 12 h');
  expect(time).toHaveTextContent('Earned on Nov 7, 2026');
  // What is next: at most two ladders already started, each with its next tier and how far along it is.
  const next = within(within(medals).getByRole('list', { name: 'Next up' })).getAllByRole('listitem');
  expect(next.length).toBeLessThanOrEqual(2);
  for (const item of next) {
    expect(item).toHaveTextContent(/Next: (Iron|Gold|Diamond) at /);
    expect(within(item).getByRole('progressbar')).toBeInTheDocument();
  }
  // Signed out: nothing is said about Minecraft achievements.
  expect(within(medals).queryByText(/Minecraft achievement/)).toBeNull();
  const key = within(medals).getByRole('button', { name: 'Medals and achievements' });
  expect(key).not.toHaveClass('btn-primary');
  await user.click(key);
  expect(routeSnapshot()).toEqual({ name: 'achievements' });
});

it('counts the Minecraft achievements on Perland servers once the accounts are linked', async () => {
  renderApp(<Profile />, { perland: 'linked', microsoft: 'signedIn' });
  expect(await screen.findByText('22 Minecraft achievements on 3 Perland servers')).toBeInTheDocument();
});

it('says so when no medal is earned yet, and still opens Medals and achievements', async () => {
  const { backend, qc } = renderApp(<Profile />);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation(async (cmd, args) => {
    const answer = await handle(cmd, args);
    if (cmd !== 'medals_view') return answer;
    const view = answer as MedalsView;
    return { ...view, medals: view.medals.map((m) => ({ ...m, tier: 0, value: 0, earnedAt: null })) };
  });
  const medals = (await screen.findByRole('heading', { name: 'Medals', level: 2 })).closest('section')!;
  // The first answer was already on its way when the stub went in. Once it has landed, ask again: an invalidation made
  // while a first read is in flight is folded into it.
  await within(medals).findByText(/of 24 medals$/);
  await act(() => qc.invalidateQueries({ queryKey: ['medals'] }));
  expect(await within(medals).findByText('No medals yet. Your first one comes with your first game.')).toBeInTheDocument();
  expect(within(medals).getByText('0 of 24 medals')).toBeInTheDocument();
  expect(within(medals).queryByRole('list')).toBeNull();
  expect(within(medals).getByRole('button', { name: 'Medals and achievements' })).toBeInTheDocument();
});

it('says how full the wardrobe and the catalog are, and opens Skins', async () => {
  const { user } = renderApp(<Profile />);
  const skin = (await screen.findByRole('heading', { name: 'Your skin' })).closest('section')!;
  expect(await within(skin).findByText(/2 in your wardrobe\./)).toBeInTheDocument();
  expect(await within(skin).findByText(/ready-made looks in the catalog\./)).toBeInTheDocument();
  expect(within(skin).getByTestId('player-figure')).toBeInTheDocument();
  await user.click(within(skin).getByRole('button', { name: 'Change skin' }));
  expect(routeSnapshot()).toEqual({ name: 'skins' });
});

it('asks an offline player to finish setting up: who is playing, then Microsoft and Perland, each with its reason and key', async () => {
  renderApp(<Profile />, { perland: 'signedOut' });
  const microsoft = await screen.findByRole('button', { name: 'Sign in with Microsoft' });
  const perland = await screen.findByRole('button', { name: 'Continue with Perland' });
  const [playing, first, second] = steps();
  expect(steps()).toHaveLength(3);
  expect(playing).toHaveTextContent('Playing as Steve: done');
  expect(playing).toHaveTextContent('Offline account');
  expect(first).toHaveAccessibleName('Microsoft account: to do');
  expect(first).toHaveTextContent('Sign in to wear your skins in Minecraft. The account must own Minecraft: Java Edition.');
  expect(first).toContainElement(microsoft);
  expect(second).toHaveAccessibleName('Perland account: to do');
  expect(second).toContainElement(perland);
  // One lit key on the page: Microsoft's. Perland's is raised.
  expect(microsoft).toHaveClass('btn-primary');
  expect(microsoft).not.toHaveClass('btn-lg');
  expect(document.querySelectorAll('.btn-primary')).toHaveLength(1);
});

it('heads its column: the checklist comes before the look at the medals', async () => {
  renderApp(<Profile />);
  const title = await screen.findByRole('heading', { name: 'Finish setting up' });
  const medals = screen.getByRole('heading', { name: 'Medals', level: 2 });
  expect(title.compareDocumentPosition(medals) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it('asks only for what is missing', async () => {
  const { unmount } = renderApp(<Profile />, { microsoft: 'signedIn', perland: 'signedOut' });
  expect(await screen.findByRole('button', { name: 'Continue with Perland' })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Sign in with Microsoft' })).toBeNull();
  expect(steps()[0]).toHaveTextContent('Playing as Alex');
  expect(steps()[0]).toHaveTextContent('Microsoft account');
  expect(steps()).toHaveLength(2);
  unmount();

  renderApp(<Profile />, { perland: 'signedIn' });
  expect(await screen.findByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Continue with Perland' })).toBeNull();
});

it('never asks for a Perland account while the Engine has accounts closed', async () => {
  const { calls } = renderApp(<Profile />, { perland: 'off' });
  expect(await screen.findByRole('button', { name: 'Sign in with Microsoft' })).toBeInTheDocument();
  await waitFor(() => expect(sent(calls, 'perland_status')).toHaveLength(1));
  await act(async () => {});
  expect(steps()).toHaveLength(2);
});

it('shows no checklist once everything is connected', async () => {
  const { calls } = renderApp(<Profile />, { microsoft: 'signedIn', perland: 'linked' });
  expect(await screen.findByRole('heading', { name: 'Alex', level: 1 })).toBeInTheDocument();
  await waitFor(() => expect(sent(calls, 'perland_status')).toHaveLength(1));
  await screen.findByRole('button', { name: 'More for your Perland account' });
  expect(setUp()).toBeNull();
  // Nothing is lit on the page then.
  expect(document.querySelector('.btn-primary')).toBeNull();
});

it('signs in with Microsoft from its step: the steps show in the panel, then the step goes and the new account has the keyboard', async () => {
  // Short steps: the sign-in's five calls must not make the test depend on how fast the machine is.
  const { user, calls } = renderApp(<Profile />, { perland: 'signedIn', microsoftStepMs: 60 });
  await user.click(await screen.findByRole('button', { name: 'Sign in with Microsoft' }));
  expect(sent(calls, 'account_sign_in_microsoft')).toEqual([{ language: 'en' }]);
  // The wait and Cancel take the step's own line.
  expect(await within(setUp()!).findByRole('button', { name: 'Cancel' })).toBeInTheDocument();
  await screen.findByRole('heading', { name: 'Alex', level: 1 }, { timeout: 4000 });
  // Signed in to Perland too, so nothing is left to set up.
  expect(setUp()).toBeNull();
  expect(screen.getByRole('status')).toHaveTextContent('Signed in as Alex.');
  await waitFor(() => expect(screen.getByRole('button', { name: 'More for Alex' })).toHaveFocus());
});
