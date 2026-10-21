import { screen } from '@testing-library/react';
import { createInstance, type TFunction } from 'i18next';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import en from '../../i18n/en.json';
import es from '../../i18n/es.json';
import type { ServerStats } from '../../ipc/schema';
import { renderApp, sent } from '../../test/backend';
import { formatPlaytime } from '../../ui/playtime';
import { ServerStatsLine, statsText } from './ServerStatsLine';

let t: TFunction;
let tEs: TFunction;

beforeAll(async () => {
  const make = async (lng: string) => {
    const i18n = createInstance();
    await i18n.init({ resources: { en: { translation: en }, es: { translation: es } }, lng, fallbackLng: 'en' });
    return i18n.t;
  };
  t = await make('en');
  tEs = await make('es');
});

/** The owner's server in the `linked` mock scenario: 12 achievements, 108000 s. */
const OWNER_TOKEN = 'O'.repeat(43);

const stats = (advancements: number, playtimeSeconds: number): ServerStats => ({
  token: OWNER_TOKEN,
  instance: null,
  name: 'My world',
  advancements,
  playtimeSeconds,
  advancementIds: [],
});

describe('statsText', () => {
  it('joins the achievements and the time played', () => {
    expect(statsText(t, stats(12, 108000))).toBe(`12 achievements · ${formatPlaytime(108000, t)} played`);
  });

  it('says one achievement alone without a minute played', () => {
    expect(statsText(t, stats(1, 0))).toBe('1 achievement');
  });

  it('is empty with nothing to say', () => {
    expect(statsText(t, stats(0, 59))).toBe('');
  });

  it('says only the time played without achievements', () => {
    expect(statsText(t, stats(0, 3600))).toBe(`${formatPlaytime(3600, t)} played`);
    expect(statsText(t, stats(0, 3600))).toBe('1 h played');
  });

  it('reads in Spanish', () => {
    expect(statsText(tEs, stats(12, 108000))).toBe(`12 logros · ${formatPlaytime(108000, tEs)} de juego`);
    expect(statsText(tEs, stats(1, 0))).toBe('1 logro');
  });
});

describe('ServerStatsLine', () => {
  it('renders the line with the linked stats', async () => {
    renderApp(<ServerStatsLine token={OWNER_TOKEN} />, { perland: 'linked' });
    const line = await screen.findByRole('button', { name: /12 achievements/ });
    expect(line).toHaveAccessibleName(
      `Your stats on this server: 12 achievements · ${formatPlaytime(108000, t)} played. See your achievements on My world`,
    );
  });

  it('opens the server’s achievements from the line', async () => {
    const { user } = renderApp(<ServerStatsLine token={OWNER_TOKEN} />, { perland: 'linked' });
    await user.click(await screen.findByRole('button', { name: /12 achievements/ }));
    const dialog = screen.getByRole('dialog', { name: 'Achievements on My world' });
    expect(dialog).toHaveTextContent('Free the End');
    await user.click(screen.getByRole('button', { name: 'Close' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('renders nothing for a server without stats', async () => {
    const { calls, container } = renderApp(<ServerStatsLine token={'Z'.repeat(43)} />, { perland: 'linked' });
    await vi.waitFor(() => expect(sent(calls, 'perland_stats')).toHaveLength(1));
    await new Promise((r) => setTimeout(r, 0));
    expect(container).toBeEmptyDOMElement();
  });

  it.each(['signedOut', 'noAddon', 'signedIn'] as const)('renders nothing with %s', async (perland) => {
    const { calls, container } = renderApp(<ServerStatsLine token={OWNER_TOKEN} />, { perland });
    await vi.waitFor(() => expect(sent(calls, 'perland_status')).toHaveLength(1));
    await new Promise((r) => setTimeout(r, 0));
    expect(container).toBeEmptyDOMElement();
  });
});
