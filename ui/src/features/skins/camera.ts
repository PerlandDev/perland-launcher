// Where the stage's camera stands: it circles a point (yaw, pitch), stands a distance from it (zoom), and the point
// can be slid off the figure's middle (pan), so the player can look closely at a hand or an ear. The wheel zooms
// toward the point under the pointer, the way a map does. Pure, so it is tested without WebGL.
import type { Vec3 } from './layout';

export interface View {
  yaw: number;
  pitch: number;
  /** The camera's distance as a share of the distance that fits the whole pose. */
  zoom: number;
  /** How far the point the camera circles is slid from the pose's own centre. */
  pan: Vec3;
}

export const START_VIEW: View = { yaw: -0.55, pitch: 0.14, zoom: 1, pan: [0, 0, 0] };
/** Close enough to fill the stage with a few texels, far enough to see a horse whole. */
export const ZOOM_MIN = 0.12;
export const ZOOM_MAX = 1.8;
/** The point the camera circles stays within this many model units of the pose's centre, so the figure can't be lost. */
export const PAN_MAX = 28;

export function startView(): View {
  return { ...START_VIEW, pan: [...START_VIEW.pan] };
}

const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const scale = (a: Vec3, k: number): Vec3 => [a[0] * k, a[1] * k, a[2] * k];

function clampPan(pan: Vec3): Vec3 {
  const length = Math.hypot(...pan);
  return length > PAN_MAX ? scale(pan, PAN_MAX / length) : pan;
}

/** The point the camera circles, for a pose centred on `centre`. */
export function targetOf(view: View, centre: Vec3): Vec3 {
  return add(centre, view.pan);
}

/** From the target toward the camera, one unit long. */
export function backward(view: View): Vec3 {
  return [Math.cos(view.pitch) * Math.sin(view.yaw), Math.sin(view.pitch), Math.cos(view.pitch) * Math.cos(view.yaw)];
}

/** The camera's own right and up, one unit long: the directions a pan slides along. */
export function screenAxes(view: View): { right: Vec3; up: Vec3 } {
  const right: Vec3 = [Math.cos(view.yaw), 0, -Math.sin(view.yaw)];
  const b = backward(view);
  // up = backward × right
  const up: Vec3 = [b[1] * right[2] - b[2] * right[1], b[2] * right[0] - b[0] * right[2], b[0] * right[1] - b[1] * right[0]];
  return { right, up };
}

/** Where the camera stands, `fitDistance` being the distance at which the whole pose fits. */
export function cameraPosition(view: View, centre: Vec3, fitDistance: number): Vec3 {
  return add(targetOf(view, centre), scale(backward(view), fitDistance * view.zoom));
}

export function orbit(view: View, dx: number, dy: number): View {
  return { ...view, yaw: view.yaw - dx * 0.01, pitch: Math.max(-1.2, Math.min(1.2, view.pitch + dy * 0.01)) };
}

/**
 * Zooms by `factor` (below 1 is closer). With `point`, the model point under the pointer, the camera moves along the
 * line through it, so that point stays under the pointer; without one it closes in on the target.
 */
export function zoomAt(view: View, factor: number, centre: Vec3, point: Vec3 | null): View {
  const zoom = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, view.zoom * factor));
  if (!point) return { ...view, zoom };
  const f = zoom / view.zoom;
  const target = add(point, scale(sub(targetOf(view, centre), point), f));
  return { ...view, zoom, pan: clampPan(sub(target, centre)) };
}

/** Slides the view by a pointer drag of (dx, dy) CSS pixels, so the figure follows the pointer. `unitsPerPixel` is how
 *  many model units one pixel spans at the target's distance. */
export function pan(view: View, dx: number, dy: number, unitsPerPixel: number): View {
  const { right, up } = screenAxes(view);
  const move = add(scale(right, -dx * unitsPerPixel), scale(up, dy * unitsPerPixel));
  return { ...view, pan: clampPan(add(view.pan, move)) };
}
