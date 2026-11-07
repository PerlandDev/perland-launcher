import { describe, expect, it } from 'vitest';
import { celebration, CELEBRATE_S, POSE_STILL_T, POSES, rigAt, SADDLE_Y } from './poses';

describe('poses', () => {
  it('walk swings the legs against each other and the arms against the legs', () => {
    const rig = rigAt('walk', POSE_STILL_T);
    expect(rig.joints.rightLeg[0]).toBeGreaterThan(0.3);
    expect(rig.joints.leftLeg[0]).toBeCloseTo(-rig.joints.rightLeg[0]);
    expect(Math.sign(rig.joints.rightArm[0])).toBe(-Math.sign(rig.joints.rightLeg[0]));
  });

  it('sit puts the legs straight out in front and the figure down on the plinth', () => {
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
