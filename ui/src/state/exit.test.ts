import { expect, it } from 'vitest';
import { exitOf } from '../ipc/mock';
import { isCrash, suggestMemory } from './exit';

const base = exitOf({ code: 0 });

it('matches the Rust definition of a crash', () => {
  expect(isCrash({ ...base, code: 1 })).toBe(true);
  expect(isCrash({ ...base, code: null })).toBe(true);
  expect(isCrash(base)).toBe(false);
  expect(isCrash({ ...base, code: null, killed: true })).toBe(false);
  expect(isCrash({ ...base, code: null, cancelled: true })).toBe(false);
});

it('suggests 2 GB more memory, within what the computer can spare', () => {
  expect(suggestMemory(4096, 16384)).toBe(6144);
  expect(suggestMemory(10240, 16384)).toBe(12288);
  expect(suggestMemory(12288, 16384)).toBeNull(); // already at 75 %
  expect(suggestMemory(2048, 4096)).toBe(3072);
});
