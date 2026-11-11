// Where the stage camera stands: the whole figure, or a close-up of the part the Create tab is dressing. A shot is
// what must fit in view (a point to look at and the height and width around it) and the angle it is seen from. Pure,
// so it is tested without WebGL; stageScene animates between shots.
import type { Vec3 } from './layout';
import { rigAt, type Pose } from './poses';

/** What the camera can frame. `upper` is head to waist; `back` is the torso from behind. */
export const FOCUSES = ['figure', 'head', 'hair', 'hat', 'upper', 'torso', 'back', 'legs', 'feet'] as const;
export type Focus = (typeof FOCUSES)[number];

/** What must fit in view: a box `height` tall and `width` wide around `target` (model units, one a texel). */
export interface Frame {
  target: Vec3;
  height: number;
  width: number;
}
/** The player's side of the camera: the angle around the target and a zoom over the frame's own distance. */
export interface View {
  yaw: number;
  pitch: number;
  zoom: number;
}
export type Shot = Frame & Pick<View, 'yaw' | 'pitch'>;
/** The stage's lens: tan of half the vertical field of view, and width over height. */
export interface Fit {
  half: number;
  aspect: number;
}

/** A camera glide's length; under reduced motion the camera jumps. */
export const GLIDE_MS = 350;

/** The Create category's focus; in Accessories, the slot of the piece picked there decides. */
export function focusFor(category: string, slot?: string): Focus {
  switch (category) {
    case 'face':
      return 'head';
    case 'hair':
      return 'hair';
    case 'hats':
      return 'hat';
    case 'tops':
      return 'torso';
    case 'bottoms':
      return 'legs';
    case 'shoes':
      return 'feet';
    case 'accessories':
      return slot === 'glasses' ? 'head' : slot === 'back' ? 'back' : slot === 'hands' || slot === 'belt' ? 'torso' : 'upper';
    default:
      return 'figure';
  }
}

/** The whole figure in each pose, with its boat or horse: the stage's view before close-ups existed. */
const FIGURE: Record<Pose, Frame> = {
  still: { target: [0, 13, 0], height: 50, width: 30 },
  walk: { target: [0, 13, 0], height: 50, width: 30 },
  run: { target: [0, 13.5, 0], height: 51, width: 32 },
  crouch: { target: [0, 12, 0], height: 48, width: 30 },
  sit: { target: [0, 8, 0], height: 42, width: 34 },
  wave: { target: [0, 15, 0], height: 54, width: 32 },
  boat: { target: [0, 8, 0], height: 44, width: 40 },
  horse: { target: [0, 25, 0], height: 72, width: 46 },
};

/** The angle the whole figure is seen from. */
export const FIGURE_ANGLE = { yaw: -0.55, pitch: 0.14 } as const;

/**
 * Each close-up on the standing figure (feet on 0, hips at 12, shoulders at 24, crown at 32): the point looked at,
 * which joint carries it, how much fits, and the angle. The face is seen nearly from the front; hair from three
 * quarters and above, where its shape shows; shoes from above, where their tops show. Each point sits a little under
 * its part's middle, so the part stands clear of the keys and notes along the foot of the stage.
 */
const CLOSE: Record<Exclude<Focus, 'figure'>, { at: Vec3; on: 'torso' | 'legs'; height: number; width: number; yaw: number; pitch: number }> = {
  head: { at: [0, 25.5, 0], on: 'torso', height: 24, width: 22, yaw: -0.3, pitch: 0.06 },
  hair: { at: [0, 26, 0], on: 'torso', height: 24, width: 22, yaw: -0.72, pitch: 0.3 },
  hat: { at: [0, 26.5, 0], on: 'torso', height: 25, width: 22, yaw: -0.5, pitch: 0.2 },
  upper: { at: [0, 20.5, 0], on: 'torso', height: 34, width: 26, yaw: -0.45, pitch: 0.1 },
  torso: { at: [0, 16.5, 0], on: 'torso', height: 32, width: 26, yaw: -0.5, pitch: 0.1 },
  back: { at: [0, 17, 0], on: 'torso', height: 34, width: 26, yaw: Math.PI - 0.5, pitch: 0.12 },
  legs: { at: [0, 5, 0], on: 'legs', height: 28, width: 22, yaw: -0.5, pitch: 0.1 },
  feet: { at: [0, 1, 0], on: 'legs', height: 18, width: 18, yaw: -0.6, pitch: 0.38 },
};

/** Poses whose prop hides the legs (down in the boat's hull, round the horse's flanks): no close-up of them there. */
const LEGS_AWAY: readonly Pose[] = ['boat', 'horse'];

/** Where the hips are: the torso and the legs both turn there. */
const HIPS: Vec3 = [0, 12, 0];

/** `point` after the joint that carries it turned by `angle` about the x axis through the hips. */
function swing(point: Vec3, angle: number): Vec3 {
  const y = point[1] - HIPS[1];
  const z = point[2] - HIPS[2];
  return [point[0], HIPS[1] + y * Math.cos(angle) - z * Math.sin(angle), HIPS[2] + y * Math.sin(angle) + z * Math.cos(angle)];
}

/**
 * The shot of `focus` while the figure is in `pose`: a close-up goes where the pose puts the part (down on the floor
 * sitting, up on the saddle, leaning in crouched), read from the pose's resting frame. Where the pose's prop hides the
 * legs, their close-ups are the whole figure.
 */
export function shotOf(focus: Focus, pose: Pose): Shot {
  if (focus === 'figure' || (LEGS_AWAY.includes(pose) && CLOSE[focus].on === 'legs')) {
    const whole = FIGURE[pose];
    return { target: [...whole.target], height: whole.height, width: whole.width, ...FIGURE_ANGLE };
  }
  const close = CLOSE[focus];
  const rig = rigAt(pose, 0);
  const at = swing(close.at, close.on === 'torso' ? rig.joints.torso[0] : rig.joints.rightLeg[0]);
  return {
    target: [at[0] + rig.offset[0], at[1] + rig.offset[1], at[2] + rig.offset[2]],
    height: close.height,
    width: close.width,
    yaw: close.yaw,
    pitch: close.pitch,
  };
}

/** How far back the camera stands for the frame to fit, whichever of its height and width is the tighter. */
export function fitDistance(frame: Pick<Frame, 'height' | 'width'>, fit: Fit): number {
  return Math.max(frame.height / 2 / fit.half, frame.width / 2 / (fit.half * fit.aspect));
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/**
 * The zoom kept within reach: out to 1.8 times the whole figure's distance whatever the shot, so a close-up can
 * always be zoomed out to the figure; in to 0.35 of the figure's distance, or closer still on a close-up.
 */
export function clampZoom(zoom: number, shot: Frame, figure: Frame, fit: Fit): number {
  const own = fitDistance(shot, fit);
  const whole = fitDistance(figure, fit);
  return clamp(zoom, Math.min(0.35 * whole, 0.6 * own) / own, (1.8 * whole) / own);
}

/**
 * Where the camera is and what it looks at. Zooming out of a close-up slides the target toward the whole figure's, so
 * by the time the figure fits it is centred, not hanging off the part that was framed.
 */
export function cameraAt(view: View, shot: Frame, figure: Frame, fit: Fit): { position: Vec3; target: Vec3 } {
  const own = fitDistance(shot, fit);
  const whole = fitDistance(figure, fit);
  const distance = own * view.zoom;
  const out = whole > own ? clamp((distance - own) / (whole - own), 0, 1) : 0;
  const target = shot.target.map((v, i) => (out === 0 ? v : v + (figure.target[i]! - v) * out)) as Vec3;
  const { yaw, pitch } = view;
  return {
    position: [target[0] + distance * Math.cos(pitch) * Math.sin(yaw), target[1] + distance * Math.sin(pitch), target[2] + distance * Math.cos(pitch) * Math.cos(yaw)],
    target,
  };
}

/** Out fast, settle soft. */
export function easeOut(k: number): number {
  return k <= 0 ? 0 : k >= 1 ? 1 : 1 - Math.pow(1 - k, 4);
}

const mix = (a: number, b: number, k: number) => (k >= 1 ? b : k <= 0 ? a : a + (b - a) * k);

export function glideFrame(from: Frame, to: Frame, k: number): Frame {
  return { target: from.target.map((v, i) => mix(v, to.target[i]!, k)) as Vec3, height: mix(from.height, to.height, k), width: mix(from.width, to.width, k) };
}

/** Turns the short way round: `to`'s yaw is taken as the same direction nearest to where the camera is. */
export function glideView(from: View, to: View, k: number): View {
  const turns = Math.round((from.yaw - to.yaw) / (Math.PI * 2));
  return { yaw: mix(from.yaw, to.yaw + turns * Math.PI * 2, k), pitch: mix(from.pitch, to.pitch, k), zoom: mix(from.zoom, to.zoom, k) };
}

/** The shot `k` of the way (0 to 1, already eased) from one to another. */
export function glideShot(from: Shot, to: Shot, k: number): Shot {
  const { yaw, pitch } = glideView({ yaw: from.yaw, pitch: from.pitch, zoom: 1 }, { yaw: to.yaw, pitch: to.pitch, zoom: 1 }, k);
  return { ...glideFrame(from, to, k), yaw, pitch };
}

/**
 * Whose the camera is. It follows the focus it is given until the player moves it by hand (orbit, zoom); from then on
 * it stays theirs until the `key` changes (another category, another tab) or they reset the view. `aimed` is what the
 * camera frames now, `wanted` what a reset goes to.
 */
export interface Following {
  readonly aimed: Focus;
  readonly wanted: Focus;
  /** A new focus arrived under `key`: `move` says whether the camera goes there. */
  focus(focus: Focus, key: string): { state: Following; move: boolean };
  /** The player moved the camera by hand. */
  moved(): Following;
  /** Reset view: back to what is wanted now, following again. */
  reset(): Following;
}

function follow(aimed: Focus, wanted: Focus, key: string | null, held: boolean): Following {
  return {
    aimed,
    wanted,
    focus(focus, next) {
      if (next !== key) return { state: follow(focus, focus, next, false), move: true };
      if (held || focus === aimed) return { state: follow(aimed, focus, key, held), move: false };
      return { state: follow(focus, focus, key, false), move: true };
    },
    moved: () => follow(aimed, wanted, key, true),
    reset: () => follow(wanted, wanted, key, false),
  };
}

export function following(): Following {
  return follow('figure', 'figure', null, false);
}
