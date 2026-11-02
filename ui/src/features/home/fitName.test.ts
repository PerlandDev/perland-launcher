import { describe, expect, it } from 'vitest';
import { nameFit } from './fitName';

describe('nameFit', () => {
  it('takes a smaller floor for a server tile', () => {
    // A tile name at 20px shrinks to 15px before a word may break.
    expect(nameFit({ available: 149, needed: 190, fontSize: 20, floorPx: 15 })).toEqual({ scale: 0.7789, floor: false });
    expect(nameFit({ available: 101, needed: 190, fontSize: 20, floorPx: 15 })).toEqual({ scale: 0.75, floor: true });
  });

  it('leaves a name whose longest word fits at full size', () => {
    expect(nameFit({ available: 300, needed: 300, fontSize: 46 })).toEqual({ scale: 1, floor: false });
    expect(nameFit({ available: 300, needed: 120, fontSize: 46 })).toEqual({ scale: 1, floor: false });
  });

  it('shrinks the type until the longest word fits, with a pixel to spare for rounding', () => {
    expect(nameFit({ available: 301, needed: 400, fontSize: 46 })).toEqual({ scale: 0.75, floor: false });
  });

  it('stops at 1.125rem and says so, so only then may a word break', () => {
    // 18 px is the floor: 36 px type may shrink to half and no further.
    expect(nameFit({ available: 101, needed: 400, fontSize: 36 })).toEqual({ scale: 0.5, floor: true });
  });

  it('measures nothing while the box has no width yet', () => {
    expect(nameFit({ available: 0, needed: 400, fontSize: 46 })).toEqual({ scale: 1, floor: false });
  });
});
