import { describe, expect, it } from 'vitest';
import { backward, cameraPosition, pan, PAN_MAX, screenAxes, startView, targetOf, ZOOM_MAX, ZOOM_MIN, zoomAt, type View } from './camera';
import type { Vec3 } from './layout';

const CENTRE: Vec3 = [0, 13, 0];
const FIT = 100;

/** Where `point` lands on the screen, as its offsets along the camera's right and up over its depth. */
function onScreen(view: View, point: Vec3): [number, number] {
  const eye = cameraPosition(view, CENTRE, FIT);
  const d: Vec3 = [point[0] - eye[0], point[1] - eye[1], point[2] - eye[2]];
  const { right, up } = screenAxes(view);
  const back = backward(view);
  const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const depth = -dot(d, back);
  return [dot(d, right) / depth, dot(d, up) / depth];
}

describe('camera', () => {
  it('starts on the pose centre, framing it whole', () => {
    const view = startView();
    expect(targetOf(view, CENTRE)).toEqual(CENTRE);
    expect(view.zoom).toBe(1);
  });

  it('zooming toward a point keeps that point under the pointer', () => {
    const view = startView();
    const hand: Vec3 = [-6, 11, 2];
    const before = onScreen(view, hand);
    const closer = zoomAt(view, 0.5, CENTRE, hand);
    expect(closer.zoom).toBe(0.5);
    const after = onScreen(closer, hand);
    expect(after[0]).toBeCloseTo(before[0], 6);
    expect(after[1]).toBeCloseTo(before[1], 6);
    // And the camera is closer to it.
    expect(targetOf(closer, CENTRE)).not.toEqual(CENTRE);
  });

  it('zooming with no point closes in on the target, and stays within its limits', () => {
    const view = startView();
    expect(zoomAt(view, 0.5, CENTRE, null)).toEqual({ ...view, zoom: 0.5 });
    expect(zoomAt(view, 0.001, CENTRE, [1, 2, 3]).zoom).toBe(ZOOM_MIN);
    expect(zoomAt(view, 1000, CENTRE, [1, 2, 3]).zoom).toBe(ZOOM_MAX);
  });

  it('a zoom held at its limit leaves the view where it is', () => {
    const atMin = { ...startView(), zoom: ZOOM_MIN };
    expect(zoomAt(atMin, 0.5, CENTRE, [5, 20, 3]).pan).toEqual([0, 0, 0]);
  });

  it('panning moves the figure with the pointer', () => {
    const view = startView();
    const before = onScreen(view, CENTRE);
    const moved = pan(view, 10, 0, 0.1);
    const after = onScreen(moved, CENTRE);
    // Dragged right: the centre now shows further right, at the same height.
    expect(after[0]).toBeGreaterThan(before[0]);
    expect(after[1]).toBeCloseTo(before[1], 6);
    const down = onScreen(pan(view, 0, 10, 0.1), CENTRE);
    expect(down[1]).toBeLessThan(before[1]);
  });

  it('panning never slides the target out of reach', () => {
    const far = pan(startView(), 100000, 0, 1);
    expect(Math.hypot(...far.pan)).toBeCloseTo(PAN_MAX, 6);
  });
});
