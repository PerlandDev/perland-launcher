import { mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it } from 'vitest';
import { nextPoll, pollServerStatus, resetServerPolls, strikesOf } from './serverPoll';

const limited = { category: 'network', code: 'invite_rate_limited', message: 'x', fix: null, detail: null, params: {} };
const ok = { status: 'online', players: null, whitelisted: false, packState: 'ready', gone: false, address: null };

describe('server status polling', () => {
  afterEach(resetServerPolls);

  it('backs off 30 → 60 → 120 s after rate limits', () => {
    expect([0, 1, 2, 3].map((n) => nextPoll(30_000, n))).toEqual([30_000, 60_000, 120_000, 120_000]);
    expect(nextPoll(60_000, 1)).toBe(120_000);
  });

  it('counts rate limits in a row and clears them after a success', async () => {
    let answer: unknown = limited;
    mockIPC((cmd) => {
      if (cmd !== 'server_status') return null;
      if (answer === limited) throw limited;
      return answer;
    });
    await expect(pollServerStatus('survival')).rejects.toMatchObject({ code: 'invite_rate_limited' });
    await expect(pollServerStatus('survival')).rejects.toMatchObject({ code: 'invite_rate_limited' });
    expect(strikesOf('survival')).toBe(2);
    answer = ok;
    await expect(pollServerStatus('survival')).resolves.toMatchObject({ status: 'online' });
    expect(strikesOf('survival')).toBe(0);
  });
});
