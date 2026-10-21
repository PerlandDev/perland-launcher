import { describe, expect, it } from 'vitest';
import type { PerlandStatus } from '../ipc/schema';
import { manageLog } from './manageLog';
import { createQueryClient, qk } from './queries';

const account = (name: string): PerlandStatus['account'] => ({
  name,
  displayName: name,
  email: `${name}@example.com`,
  discord: false,
  github: false,
  newPlayer: false,
  minecraft: null,
});
const status = (who: PerlandStatus['account']): PerlandStatus => ({ enabled: true, methods: ['email'], account: who });

describe('manageLog', () => {
  it('clear empties one server and keeps line numbers moving', () => {
    manageLog.push('t', 's', ['a', 'b']);
    manageLog.push('t', 'other', ['x']);
    manageLog.end('t', 's');
    manageLog.clear('t', 's');
    expect(manageLog.get('t', 's')).toEqual({ lines: [], first: 2, ended: false });
    expect(manageLog.get('t', 'other').lines).toEqual(['x']);
  });

  it('is emptied when the Perland account signs out or changes, not on a refresh of the same one', () => {
    const qc = createQueryClient();
    qc.setQueryData(qk.perland, status(account('ana')));
    manageLog.push('t', 's', ['a']);
    qc.setQueryData(qk.perland, status(account('ana')));
    expect(manageLog.get('t', 's').lines).toEqual(['a']);
    qc.setQueryData(qk.perland, status(null));
    expect(manageLog.get('t', 's').lines).toEqual([]);
    qc.setQueryData(qk.perland, status(account('ana')));
    manageLog.push('t', 's', ['b']);
    qc.setQueryData(qk.perland, status(account('bo')));
    expect(manageLog.get('t', 's').lines).toEqual([]);
  });
});
