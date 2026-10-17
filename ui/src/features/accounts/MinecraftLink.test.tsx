import { act, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MOCK_LINK_CODE, MOCK_LINK_POLLS } from '../../ipc/mockPerland';
import { qk } from '../../state/queries';
import { renderApp, sent } from '../../test/backend';
import { ServerStatsLine } from '../servers-list/ServerStatsLine';
import { MinecraftLink } from './MinecraftLink';
import { PerlandAccountCard } from './PerlandAccountCard';

const COMMAND = `/perland link ${MOCK_LINK_CODE}`;
const row = () => screen.getByRole('region', { name: 'Minecraft account' });
/** Let the poll's and the countdown's intervals run, and the answers they wait on settle. */
const advance = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

const refusal = (code: string, message = 'nope') => ({ category: 'network', code, message, fix: null, detail: null, params: {} });
/** The owner's server, which the mock's linked stats give 12 achievements. */
const OWNER_TOKEN = 'O'.repeat(43);
/** Let every answer in flight settle. */
const settle = () => act(() => new Promise((resolve) => setTimeout(resolve, 50)));

/** Answer `cmd` with `answer` instead of the mock's own. */
function stub(backend: ReturnType<typeof renderApp>['backend'], cmd: string, answer: () => Promise<unknown>) {
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((c, args) => (c === cmd ? answer() : handle(c, args)));
}

async function showCode(user: ReturnType<typeof renderApp>['user']) {
  await user.click(await screen.findByRole('button', { name: 'Link Minecraft account' }));
  return screen.findByText(MOCK_LINK_CODE);
}

describe('MinecraftLink', () => {
  // Only the intervals and the clock are fake: the mock backend, React Query and user-event keep real timeouts.
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval', 'Date'] });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('hidden when stats is null (the add-on is off)', async () => {
    const { calls } = renderApp(<MinecraftLink />, { perland: 'noAddon' });
    await vi.waitFor(() => expect(sent(calls, 'perland_stats')).toHaveLength(1));
    await act(async () => {});
    expect(screen.queryByText('Minecraft account')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Link Minecraft account' })).not.toBeInTheDocument();
  });

  it('hidden when stats fails', async () => {
    const { calls, backend } = renderApp(<MinecraftLink />, { perland: 'signedIn' });
    stub(backend, 'perland_stats', () => Promise.reject(refusal('engine_unreachable')));
    await vi.waitFor(() => expect(sent(calls, 'perland_stats')).toHaveLength(1));
    await act(async () => {});
    expect(screen.queryByText('Minecraft account')).not.toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('hidden when signed out', async () => {
    const { calls } = renderApp(<MinecraftLink />, { perland: 'signedOut' });
    await vi.waitFor(() => expect(sent(calls, 'perland_status')).toHaveLength(1));
    await act(async () => {});
    expect(sent(calls, 'perland_stats')).toHaveLength(0);
    expect(screen.queryByText('Minecraft account')).not.toBeInTheDocument();
  });

  it('sits in the signed-in Perland card, under the profile', async () => {
    renderApp(<PerlandAccountCard />, { perland: 'signedIn' });
    const link = await screen.findByRole('button', { name: 'Link Minecraft account' });
    const card = screen.getByRole('region', { name: 'Perland account' });
    expect(within(card).getByRole('region', { name: 'Minecraft account' })).toContainElement(link);
    expect(within(card).getByText('Link your Minecraft account to see your achievements on each server.')).toBeInTheDocument();
    const signOut = within(card).getByRole('button', { name: 'Sign out' });
    expect(signOut.compareDocumentPosition(link) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('links with a code', async () => {
    const { user, calls, qc } = renderApp(<MinecraftLink />, { perland: 'signedIn' });
    const invalidate = vi.spyOn(qc, 'invalidateQueries');
    await showCode(user);
    expect(sent(calls, 'perland_link_code')).toHaveLength(1);
    expect(within(row()).getByText(COMMAND).closest('p')).toHaveTextContent(`Type ${COMMAND} on a Perland server.`);
    // Moving to the code puts the player on its first control.
    const copy = screen.getByRole('button', { name: 'Copy command' });
    expect(copy).toHaveFocus();

    await user.click(copy);
    expect(await navigator.clipboard.readText()).toBe(COMMAND);
    expect(await screen.findByRole('button', { name: 'Copied' })).toBeInTheDocument();
    expect(screen.getByText('Command copied')).toHaveAttribute('aria-live', 'polite');

    for (let i = 0; i < MOCK_LINK_POLLS; i++) await advance(3000);
    expect(sent(calls, 'perland_link_poll')).toHaveLength(MOCK_LINK_POLLS);
    expect(await within(row()).findByText(/Linked to/)).toHaveTextContent('Linked to Steve');
    expect(screen.getByText('Your Minecraft account Steve is linked.')).toHaveAttribute('aria-live', 'polite');
    expect(screen.getByRole('button', { name: 'Unlink' })).toHaveFocus();
    expect(screen.queryByText(MOCK_LINK_CODE)).not.toBeInTheDocument();
    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.perland });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.perlandStats, refetchType: 'none' });

    // Linked: no more polling.
    await advance(9000);
    expect(sent(calls, 'perland_link_poll')).toHaveLength(MOCK_LINK_POLLS);
  });

  it('counts down and expires', async () => {
    const { user, calls, backend } = renderApp(<MinecraftLink />, { perland: 'signedIn' });
    // The add-on turned off mid-link: the poll keeps answering null and the code runs out normally.
    stub(backend, 'perland_link_poll', () => Promise.resolve(null));
    await showCode(user);
    expect(within(row()).getByText('Expires in 10:00')).toBeInTheDocument();
    await advance(1000);
    expect(within(row()).getByText('Expires in 9:59')).toBeInTheDocument();

    await advance(599_000);
    expect(await within(row()).findByText('The code expired.')).toBeInTheDocument();
    expect(screen.queryByText(MOCK_LINK_CODE)).not.toBeInTheDocument();
    const again = screen.getByRole('button', { name: 'Get a new code' });
    expect(again).toHaveFocus();
    // A poll every 3 s for the code's 10 minutes, then none.
    const polls = sent(calls, 'perland_link_poll').length;
    expect(polls).toBeGreaterThanOrEqual(199);
    await advance(9000);
    expect(sent(calls, 'perland_link_poll')).toHaveLength(polls);

    await user.click(again);
    expect(await screen.findByText(MOCK_LINK_CODE)).toBeInTheDocument();
    expect(sent(calls, 'perland_link_code')).toHaveLength(2);
    expect(within(row()).getByText('Expires in 10:00')).toBeInTheDocument();
  });

  it('cancel goes back and stops polling', async () => {
    const { user, calls } = renderApp(<MinecraftLink />, { perland: 'signedIn' });
    await showCode(user);
    await advance(3000);
    expect(sent(calls, 'perland_link_poll')).toHaveLength(1);
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.getByRole('button', { name: 'Link Minecraft account' })).toHaveFocus();
    expect(screen.queryByText(MOCK_LINK_CODE)).not.toBeInTheDocument();
    await advance(9000);
    expect(sent(calls, 'perland_link_poll')).toHaveLength(1);
  });

  it('ignores a poll answer that lands after Cancel', async () => {
    const { user, backend } = renderApp(<MinecraftLink />, { perland: 'signedIn' });
    let answer: (name: string | null) => void = () => {};
    stub(backend, 'perland_link_poll', () => new Promise((resolve) => (answer = resolve)));
    await showCode(user);
    await advance(3000);
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await act(async () => answer('Steve'));
    expect(screen.getByRole('button', { name: 'Link Minecraft account' })).toBeInTheDocument();
    expect(screen.queryByText(/Linked to/)).not.toBeInTheDocument();
  });

  it('a link the status reads first ends the code, and Unlink starts from the pitch', async () => {
    const { user, qc, backend } = renderApp(<MinecraftLink />, { perland: 'signedIn' });
    stub(backend, 'perland_link_poll', () => Promise.resolve(null));
    await showCode(user);
    // Typed in game and picked up by the status before the next poll answers.
    const account = backend.state.perland.account;
    if (account) backend.state.perland.account = { ...account, minecraft: 'Steve' };
    await act(() => qc.invalidateQueries({ queryKey: qk.perland }));
    expect(await within(row()).findByText(/Linked to/)).toHaveTextContent('Linked to Steve');
    expect(screen.queryByText(MOCK_LINK_CODE)).not.toBeInTheDocument();
    expect(screen.getByText('Your Minecraft account Steve is linked.')).toHaveAttribute('aria-live', 'polite');

    await user.click(screen.getByRole('button', { name: 'Unlink' }));
    expect(await screen.findByRole('button', { name: 'Link Minecraft account' })).toHaveFocus();
    expect(screen.getByText('Link your Minecraft account to see your achievements on each server.')).toBeInTheDocument();
    expect(screen.queryByText(MOCK_LINK_CODE)).not.toBeInTheDocument();
    expect(screen.queryByText('The code expired.')).not.toBeInTheDocument();
  });

  it('unlink shows the link button again', async () => {
    const { user, calls, qc } = renderApp(<MinecraftLink />, { perland: 'linked' });
    const invalidate = vi.spyOn(qc, 'invalidateQueries');
    expect(await within(await screen.findByRole('region', { name: 'Minecraft account' })).findByText(/Linked to/)).toHaveTextContent(
      'Linked to Steve',
    );
    await user.click(screen.getByRole('button', { name: 'Unlink' }));
    const link = await screen.findByRole('button', { name: 'Link Minecraft account' });
    expect(link).toHaveFocus();
    expect(sent(calls, 'perland_unlink')).toHaveLength(1);
    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.perland });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.perlandStats, refetchType: 'none' });
  });

  it('a sign-out the status sees after a link leaves no stats on the card', async () => {
    const { user, qc, backend } = renderApp(
      <>
        <MinecraftLink />
        <ServerStatsLine token={OWNER_TOKEN} />
      </>,
      { perland: 'signedIn' },
    );
    await showCode(user);
    for (let i = 0; i < MOCK_LINK_POLLS; i++) await advance(3000);
    expect(await screen.findByText(/12 achievements/)).toBeInTheDocument();
    // The session ends on the Engine and the status reads it, without the card's own sign-out.
    backend.state.perland.account = null;
    await act(() => qc.invalidateQueries({ queryKey: qk.perland }));
    await settle();
    expect(screen.queryByText(/\d+ achievement/)).toBeNull();
    expect(screen.queryByRole('region', { name: 'Minecraft account' })).not.toBeInTheDocument();
  });

  it('right after Unlink, the card shows no numbers from the linked stats', async () => {
    const { user, backend } = renderApp(
      <>
        <MinecraftLink />
        <ServerStatsLine token={OWNER_TOKEN} />
      </>,
      { perland: 'signedIn' },
    );
    await showCode(user);
    for (let i = 0; i < MOCK_LINK_POLLS; i++) await advance(3000);
    expect(await screen.findByText(/12 achievements/)).toBeInTheDocument();
    // The stats read after Unlink is slow: until it answers, the card shows what the cache holds for "not linked".
    stub(backend, 'perland_stats', () => new Promise(() => {}));
    await user.click(screen.getByRole('button', { name: 'Unlink' }));
    await screen.findByRole('button', { name: 'Link Minecraft account' });
    expect(screen.queryByText(/\d+ achievement/)).toBeNull();
    await settle();
    expect(screen.queryByText(/\d+ achievement/)).toBeNull();
  });

  it.each(['Link Minecraft account', 'Unlink'])('a sign-out refusal from %s reads the status again', async (name) => {
    const { user, qc, backend } = renderApp(<MinecraftLink />, { perland: name === 'Unlink' ? 'linked' : 'signedIn' });
    const invalidate = vi.spyOn(qc, 'invalidateQueries');
    const refuse = () => {
      backend.state.perland.account = null;
      return Promise.reject({ ...refusal('perland_signed_out'), category: 'auth' });
    };
    stub(backend, name === 'Unlink' ? 'perland_unlink' : 'perland_link_code', refuse);
    await user.click(await screen.findByRole('button', { name }));
    await vi.waitFor(() => expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.perland }));
    await settle();
    expect(screen.queryByRole('region', { name: 'Minecraft account' })).not.toBeInTheDocument();
  });

  it('link code refusal shows inline', async () => {
    const { user, backend } = renderApp(<MinecraftLink />, { perland: 'signedIn' });
    stub(backend, 'perland_link_code', () => Promise.reject(refusal('minecraft_link_closed')));
    await user.click(await screen.findByRole('button', { name: 'Link Minecraft account' }));
    expect(await within(row()).findByRole('alert')).toHaveTextContent("Linking isn't available right now.");
    expect(screen.getByRole('button', { name: 'Link Minecraft account' })).toBeInTheDocument();
  });

  it('a sign-out during the code stops polling', async () => {
    const { user, calls, qc, backend } = renderApp(<MinecraftLink />, { perland: 'signedIn' });
    const invalidate = vi.spyOn(qc, 'invalidateQueries');
    await showCode(user);
    stub(backend, 'perland_link_poll', () => Promise.reject(refusal('perland_signed_out')));
    await advance(3000);
    expect(sent(calls, 'perland_link_poll')).toHaveLength(1);
    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.perland });
    expect(screen.queryByText(MOCK_LINK_CODE)).not.toBeInTheDocument();
    await advance(9000);
    expect(sent(calls, 'perland_link_poll')).toHaveLength(1);
  });

  it('keeps polling through a failed poll', async () => {
    const { user, calls, backend } = renderApp(<MinecraftLink />, { perland: 'signedIn' });
    await showCode(user);
    let polled = 0;
    stub(backend, 'perland_link_poll', () => (++polled === 1 ? Promise.reject(refusal('engine_unreachable')) : Promise.resolve(null)));
    await advance(3000);
    expect(screen.getByText(MOCK_LINK_CODE)).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await advance(3000);
    expect(sent(calls, 'perland_link_poll')).toHaveLength(2);
  });
});
