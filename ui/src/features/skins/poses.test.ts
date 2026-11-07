import { describe, expect, it } from 'vitest';
import { AnimationClock, ARM_REACH, celebration, handleAt, paddleAt, CELEBRATE_S, POSE_STILL_T, POSES, rigAt, SADDLE_Y } from './poses';

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

  it('the clock stops while paused and goes on from the same moment', () => {
    const clock = new AnimationClock(1000);
    expect(clock.at(3000)).toBeCloseTo(2);
    clock.pause(3000);
    expect(clock.at(9000)).toBeCloseTo(2);
    // Pausing twice changes nothing.
    clock.pause(9500);
    expect(clock.at(9500)).toBeCloseTo(2);
    clock.resume(10000);
    expect(clock.at(10500)).toBeCloseTo(2.5);
    clock.resume(11000);
    expect(clock.at(11000)).toBeCloseTo(3);
    expect(clock.paused).toBe(false);
  });

  it('rowing keeps both hands on the paddle handles through the stroke', () => {
    for (const t of [0, 0.3, 0.7, 1.1, 1.6]) {
      const rig = rigAt('boat', t);
      for (const side of [-1, 1] as const) {
        const arm = side < 0 ? rig.joints.rightArm : rig.joints.leftArm;
        const [rx, , rz] = arm;
        const lean = rig.joints.torso[0];
        // The hand in the torso's frame: the shoulder, then ARM_REACH down the turned arm (x, then z).
        const local = [side * 6, 10 + -ARM_REACH * Math.cos(rz) * Math.cos(rx), -ARM_REACH * Math.cos(rz) * Math.sin(rx)];
        const lx = local[0]! + ARM_REACH * Math.sin(rz);
        // Then the torso's lean round its pivot (12 up), then the figure's place.
        const hand = [
          lx + rig.offset[0],
          12 + local[1]! * Math.cos(lean) - local[2]! * Math.sin(lean) + rig.offset[1],
          local[1]! * Math.sin(lean) + local[2]! * Math.cos(lean) + rig.offset[2],
        ];
        const grip = handleAt(t, side);
        expect(Math.hypot(hand[0]! - grip[0], hand[1]! - grip[1], hand[2]! - grip[2]), `t=${t} side=${side}`).toBeLessThan(1.5);
      }
    }
  });

  it('the paddles sweep back and forth together and dip their blades', () => {
    const a = paddleAt(0.2, -1);
    const b = paddleAt(1.0, -1);
    expect(a.yaw).not.toBeCloseTo(b.yaw);
    expect(paddleAt(0.2, 1).yaw).toBeCloseTo(-a.yaw);
    expect(a.dip).toBeLessThan(0);
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
