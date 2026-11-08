import { describe, expect, it } from 'vitest';
import { celebration, CELEBRATE_S, POSE_STILL_T, POSES, rigAt, SADDLE_Y, STRAIGHT } from './poses';

describe('poses', () => {
  it('walk swings the legs against each other and the arms against the legs', () => {
    const rig = rigAt('walk', POSE_STILL_T);
    expect(rig.joints.rightLeg[0]).toBeGreaterThan(0.3);
    expect(rig.joints.leftLeg[0]).toBeCloseTo(-rig.joints.rightLeg[0]);
    expect(Math.sign(rig.joints.rightArm[0])).toBe(-Math.sign(rig.joints.rightLeg[0]));
  });

  it('sit puts the legs straight out in front and the figure down on the floor', () => {
    const rig = rigAt('sit', 0);
    expect(rig.joints.rightLeg[0]).toBeCloseTo(-Math.PI / 2);
    expect(rig.offset[1]).toBe(-9.5);
  });

  it('horse seats the rider on the saddle with legs apart', () => {
    const rig = rigAt('horse', 0);
    expect(rig.offset[1]).toBeCloseTo(SADDLE_Y - 12);
    expect(rig.joints.rightLeg[2]).toBeLessThan(0);
    expect(rig.joints.leftLeg[2]).toBeGreaterThan(0);
  });

  it('standing breathes and looks around', () => {
    expect(rigAt('still', 0)).not.toEqual(rigAt('still', 1.5));
  });

  it('standing turns the arms out from the shoulder, so their tops sink into the torso', () => {
    // Why the straight pose exists: the arms turn about a point below their top edge.
    const { joints } = rigAt('still', 0);
    expect(joints.rightArm[2]).not.toBe(0);
    expect(joints.leftArm[2]).not.toBe(0);
  });

  it('the straight pose holds every joint square, the figure on its spot, nothing moving', () => {
    for (const rotation of Object.values(STRAIGHT.joints)) expect(rotation).toEqual([0, 0, 0]);
    expect(STRAIGHT.offset).toEqual([0, 0, 0]);
    expect(STRAIGHT.tilt).toEqual([0, 0, 0]);
  });

  it('in the boat the hands rest on the rim, not in the air', () => {
    for (const t of [0, 0.4, 1.3, 2.9]) {
      const rig = rigAt('boat', t);
      expect(rig.joints.torso).toEqual([0, 0, 0]);
      for (const side of [-1, 1]) {
        const [rx, , rz] = side < 0 ? rig.joints.rightArm : rig.joints.leftArm;
        // The hand's end, 10 below the shoulder pivot (side × 6, 22) along the turned arm (x, then z).
        const x = side * 6 + 10 * Math.sin(rz) + rig.offset[0];
        const y = 22 - 10 * Math.cos(rz) * Math.cos(rx) + rig.offset[1];
        const z = -10 * Math.cos(rz) * Math.sin(rx) + rig.offset[2];
        // The rim runs along each side at x = ±7 (2.4 wide), its top at y = 6.
        expect(Math.abs(x)).toBeGreaterThan(6.2);
        expect(Math.abs(x)).toBeLessThan(7.8);
        expect(y).toBeGreaterThan(6);
        expect(y).toBeLessThan(6.8);
        expect(Math.abs(z)).toBeLessThan(8);
      }
    }
  });

  it('wave raises the right arm above the head', () => {
    expect(rigAt('wave', 0).joints.rightArm[2]).toBeLessThan(-2);
  });

  it('every pose gives finite numbers at any time', () => {
    for (const pose of POSES) {
      for (const t of [0, 0.5, 3.7, 100]) {
        const rig = rigAt(pose, t);
        for (const v of [...Object.values(rig.joints).flat(), ...rig.offset, ...rig.tilt]) expect(Number.isFinite(v)).toBe(true);
      }
    }
  });

  it('the celebration turns once and lands', () => {
    expect(celebration(-0.1)).toBeNull();
    expect(celebration(CELEBRATE_S)).toBeNull();
    const mid = celebration(CELEBRATE_S / 3)!;
    expect(mid.lift).toBeGreaterThan(0);
    expect(celebration(CELEBRATE_S * 0.999)!.spin).toBeGreaterThan(Math.PI * 1.95);
  });
});
