import { describe, expect, it } from 'vitest';
import { dropIndex, moveItem } from './reorder';

describe('moveItem', () => {
  const ids = ['a', 'b', 'c', 'd'];

  it('moves the first item to the end', () => {
    expect(moveItem(ids, 0, 3)).toEqual(['b', 'c', 'd', 'a']);
  });

  it('moves the last item to the front', () => {
    expect(moveItem(ids, 3, 0)).toEqual(['d', 'a', 'b', 'c']);
  });

  it('moves one step down and up', () => {
    expect(moveItem(ids, 1, 2)).toEqual(['a', 'c', 'b', 'd']);
    expect(moveItem(ids, 2, 1)).toEqual(['a', 'c', 'b', 'd']);
  });

  it('leaves the order alone for the same index and returns a copy', () => {
    const moved = moveItem(ids, 2, 2);
    expect(moved).toEqual(ids);
    expect(moved).not.toBe(ids);
  });

  it('clamps an out-of-range target and ignores an out-of-range source', () => {
    expect(moveItem(ids, 0, 9)).toEqual(['b', 'c', 'd', 'a']);
    expect(moveItem(ids, 0, -1)).toEqual(ids);
    expect(moveItem(ids, 7, 0)).toEqual(ids);
  });

  it('does not change its input', () => {
    const input = ['a', 'b'];
    moveItem(input, 0, 1);
    expect(input).toEqual(['a', 'b']);
  });
});

describe('dropIndex', () => {
  const midpoints = [20, 60, 100];

  it('is 0 above the first midpoint', () => {
    expect(dropIndex(midpoints, 0)).toBe(0);
    expect(dropIndex(midpoints, 19)).toBe(0);
  });

  it('is n below the last midpoint', () => {
    expect(dropIndex(midpoints, 101)).toBe(3);
    expect(dropIndex(midpoints, 500)).toBe(3);
  });

  it('is the slot between the midpoints around the pointer', () => {
    expect(dropIndex(midpoints, 21)).toBe(1);
    expect(dropIndex(midpoints, 59)).toBe(1);
    expect(dropIndex(midpoints, 61)).toBe(2);
  });

  it('is 0 for an empty list', () => {
    expect(dropIndex([], 50)).toBe(0);
  });
});
