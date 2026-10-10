import { describe, expect, it } from 'vitest';
import { createRunStore, LOG_CAP } from './runStore';

const progress = (done: number) =>
  ({
    type: 'progress',
    instance: 'pack',
    task: { key: 'install_minecraft', params: { version: '1.20.1' } },
    done,
    total: 10,
    bytes: done * 100,
  }) as const;

describe('runStore', () => {
  it('follows a run from queued to idle', () => {
    const store = createRunStore();
    expect(store.run('pack').state).toBe('idle');
    store.apply({ type: 'run_state', instance: 'pack', state: 'queued', exit: null });
    store.apply({ type: 'run_state', instance: 'pack', state: 'preparing', exit: null });
    store.apply(progress(4));
    expect(store.run('pack').progress?.done).toBe(4);
    store.apply({ type: 'run_state', instance: 'pack', state: 'running', exit: null });
    expect(store.run('pack').progress).toBeNull();
    expect(store.active()).toEqual(['pack']);
    const exit = { code: 1, killed: false, cancelled: false, crash: 'out_of_memory', error: null } as const;
    store.apply({ type: 'run_state', instance: 'pack', state: 'idle', exit });
    expect(store.run('pack')).toMatchObject({ state: 'idle', lastExit: exit });
    expect(store.active()).toEqual([]);
    store.dismissExit('pack');
    expect(store.run('pack').lastExit).toBeNull();
  });

  it('starts a fresh log on every new run and caps it', () => {
    const store = createRunStore();
    store.apply({ type: 'log', instance: 'pack', lines: ['old'] });
    store.apply({ type: 'run_state', instance: 'pack', state: 'queued', exit: null });
    expect(store.log('pack')).toEqual([]);
    const many = Array.from({ length: LOG_CAP + 10 }, (_, i) => `l${i}`);
    store.apply({ type: 'log', instance: 'pack', lines: many });
    expect(store.log('pack')).toHaveLength(LOG_CAP);
    expect(store.log('pack')[0]).toBe('l10');
  });

  it('keeps snapshots stable between unrelated updates', () => {
    const store = createRunStore();
    const before = store.run('a');
    store.apply({ type: 'log', instance: 'b', lines: ['x'] });
    expect(store.run('a')).toBe(before);
    expect(store.log('missing')).toBe(store.log('also-missing'));
  });

  it('hydrates after a reload', () => {
    const store = createRunStore();
    store.hydrate([{ instance: 'pack', state: 'running' }], { pack: ['a', 'b'] });
    expect(store.run('pack').state).toBe('running');
    expect(store.log('pack')).toEqual(['a', 'b']);
  });

  it('does not let a late snapshot overwrite a state the store already heard live', () => {
    const store = createRunStore();
    store.apply({ type: 'run_state', instance: 'pack', state: 'idle', exit: null });
    store.hydrate([{ instance: 'pack', state: 'running' }, { instance: 'other', state: 'running' }], {});
    expect(store.run('pack').state).toBe('idle');
    expect(store.run('other').state).toBe('running');
  });

  it('keeps live log lines when an older tail arrives afterwards', () => {
    const store = createRunStore();
    store.apply({ type: 'log', instance: 'pack', lines: ['live 1', 'live 2'] });
    store.hydrate([], { pack: ['old', 'live 1'], other: ['x'] });
    expect(store.log('pack')).toEqual(['live 1', 'live 2']);
    expect(store.log('other')).toEqual(['x']);
  });

  it('forgets what it heard live on reset', () => {
    const store = createRunStore();
    store.apply({ type: 'run_state', instance: 'pack', state: 'idle', exit: null });
    store.reset();
    store.hydrate([{ instance: 'pack', state: 'running' }], {});
    expect(store.run('pack').state).toBe('running');
  });
});
