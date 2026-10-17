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

beforeAll(async () => {
  const i18n = createInstance();
  await i18n.init({ resources: { en: { translation: en }, es: { translation: es } }, lng: 'en', fallbackLng: 'en' });
  t = i18n.t;
});

const stats = (advancements: number, playtimeSeconds: number): ServerStats => ({
  token: 'O'.repeat(43),
  instance: null,
  name: 'My world',
  advancements,
  playtimeSeconds,
});

/** The owner's server in the `linked` mock scenario: 12 achievements, 108000 s. */
const OWNER_TOKEN = 'O'.repeat(43);

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
});

describe('ServerStatsLine', () => {
  it('renders the line with the linked stats', async () => {
    renderApp(<ServerStatsLine token={OWNER_TOKEN} />, { perland: 'linked' });
    const line = await screen.findByText(/12 achievements/);
    expect(line.closest('p')).toHaveTextContent(`Your stats on this server: 12 achievements · ${formatPlaytime(108000, t)} played`);
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
