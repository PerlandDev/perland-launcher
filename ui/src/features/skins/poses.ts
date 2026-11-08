// How the figure moves on the stage: each pose gives every joint's rotation (radians, x then y then z) and the figure's
// offset at time t (seconds). The figure faces +z: a negative x rotation swings a limb forward. Pure, so it is tested
// without WebGL. The straight pose (STRAIGHT) stands the figure square for painting, with Animation off.
import type { Vec3 } from './layout';

export const POSES = ['still', 'walk', 'run', 'crouch', 'sit', 'wave', 'boat', 'horse'] as const;
export type Pose = (typeof POSES)[number];

export type Joint = 'torso' | 'head' | 'rightArm' | 'leftArm' | 'rightLeg' | 'leftLeg';

export interface Rig {
  joints: Record<Joint, Vec3>;
  /** Where the whole figure stands, relative to its spot on the stage floor. */
  offset: Vec3;
  /** The whole figure's tilt (a rocking boat). */
  tilt: Vec3;
}

/** The rider's hips sit this high on the horse's back. */
export const SADDLE_Y = 27;

const zero = (): Vec3 => [0, 0, 0];

/** The figure stood straight for painting: every joint square, nothing turned into anything else (standing, the arms
 *  turn out about a point below their top, so their tops sink into the torso), and nothing moving. */
export const STRAIGHT: Rig = {
  joints: { torso: zero(), head: zero(), rightArm: zero(), leftArm: zero(), rightLeg: zero(), leftLeg: zero() },
  offset: zero(),
  tilt: zero(),
};

export function rigAt(pose: Pose, t: number): Rig {
  const joints: Record<Joint, Vec3> = {
    torso: zero(),
    head: zero(),
    rightArm: zero(),
    leftArm: zero(),
    rightLeg: zero(),
    leftLeg: zero(),
  };
  const offset = zero();
  const tilt = zero();
  const breathe = Math.sin(t * 1.6) * 0.03;
  switch (pose) {
    case 'still':
      joints.rightArm = [0, 0, -0.06 - breathe];
      joints.leftArm = [0, 0, 0.06 + breathe];
      joints.head = [Math.sin(t * 0.7) * 0.04, Math.sin(t * 0.45) * 0.12, 0];
      break;
    case 'walk':
    case 'run': {
      const run = pose === 'run';
      const s = Math.sin(t * Math.PI * 2 * (run ? 1.7 : 1.05));
      const leg = (run ? 0.95 : 0.6) * s;
      const arm = (run ? 0.85 : 0.5) * s;
      joints.rightLeg = [leg, 0, 0];
      joints.leftLeg = [-leg, 0, 0];
      joints.rightArm = [-arm, 0, -0.05];
      joints.leftArm = [arm, 0, 0.05];
      if (run) {
        joints.torso = [0.18, 0, 0];
        joints.head = [-0.15, 0, 0];
        offset[1] = Math.abs(s) * 0.7;
      }
      break;
    }
    case 'crouch': {
      // Leaning in over bent legs, the way the game sneaks: a slow idle bob.
      joints.torso = [0.5, 0, 0];
      joints.head = [-0.45 + Math.sin(t * 1.4) * 0.04, 0, 0];
      joints.rightArm = [-0.25, 0, -0.08];
      joints.leftArm = [-0.25, 0, 0.08];
      joints.rightLeg = [0.12, 0, 0];
      joints.leftLeg = [0.12, 0, 0];
      offset[1] = -1.5 + Math.sin(t * 1.4) * 0.15;
      offset[2] = -1.5;
      break;
    }
    case 'sit':
      // Sitting on the floor, legs out, hands on the knees, looking around.
      joints.rightLeg = [-Math.PI / 2, 0.12, 0];
      joints.leftLeg = [-Math.PI / 2, -0.12, 0];
      joints.rightArm = [-0.62, 0, -0.08];
      joints.leftArm = [-0.62, 0, 0.08];
      joints.head = [0.05, Math.sin(t * 0.5) * 0.35, 0];
      offset[1] = -9.5;
      break;
    case 'wave':
      joints.rightArm = [0, 0, -2.55 + Math.sin(t * 9) * 0.3];
      joints.leftArm = [0, 0, 0.06];
      joints.head = [-0.08, -0.15, -0.08];
      break;
    case 'boat': {
      // Sitting in the boat, legs out in front, hands resting on the rim either side while it rocks. Seated high
      // enough that the feet stay inside the hull (there's no water to hide them), and still on the seat, so the
      // hands stay on the rim as the boat moves.
      joints.rightLeg = [-1.35, 0.15, 0];
      joints.leftLeg = [-1.35, -0.15, 0];
      joints.rightArm = [-0.15, 0, -0.1];
      joints.leftArm = [-0.15, 0, 0.1];
      joints.head = [0, Math.sin(t * 0.5) * 0.3, 0];
      offset[1] = -5.5;
      tilt[2] = Math.sin(t * 1.1) * 0.05;
      tilt[0] = Math.sin(t * 0.9) * 0.025;
      break;
    }
    case 'horse': {
      // Astride, legs around the horse's flanks, hands on the reins, bouncing with its walk.
      const gait = Math.sin(t * Math.PI * 2 * 1.05);
      joints.rightLeg = [-1.05, 0, -0.42];
      joints.leftLeg = [-1.05, 0, 0.42];
      joints.rightArm = [-0.75 + gait * 0.05, 0, -0.1];
      joints.leftArm = [-0.75 + gait * 0.05, 0, 0.1];
      joints.torso = [0.06, 0, 0];
      offset[1] = SADDLE_Y - 12 + Math.abs(gait) * 0.6;
      offset[2] = -2;
      break;
    }
  }
  return { joints, offset, tilt };
}

/** A short celebration when a skin goes on: a hop and a full turn over `CELEBRATE_S` seconds. */
export const CELEBRATE_S = 0.9;

export function celebration(elapsed: number): { lift: number; spin: number } | null {
  if (elapsed < 0 || elapsed >= CELEBRATE_S) return null;
  const k = elapsed / CELEBRATE_S;
  // Out fast, settle soft: an exponential ease-out on the turn, a single arc for the hop.
  const spin = (1 - Math.pow(2, -10 * k)) * Math.PI * 2;
  const lift = Math.sin(Math.min(1, k * 1.6) * Math.PI) * 5;
  return { lift, spin };
}
