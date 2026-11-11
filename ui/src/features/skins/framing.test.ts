import { describe, expect, it } from 'vitest';
import { backward, screenAxes, startView, ZOOM_MAX, ZOOM_MIN, type View } from './camera';
import type { Vec3 } from './layout';
import { POSES } from './poses';
import { cameraAt, centreAt, clampZoom, easeOut, fitDistance, focusFor, FOCUSES, following, glideShot, glideView, GLIDE_MS, shotOf, zoomShot, type Fit, type Frame, type Shot } from './framing';

const FIT: Fit = { half: Math.tan((32 * Math.PI) / 360), aspect: 1 };
/** A view from straight ahead, not slid. */
const ahead = (zoom: number): View => ({ yaw: 0, pitch: 0, zoom, pan: [0, 0, 0] });

describe('focusFor', () => {
  it('frames the part each Create category dresses', () => {
    expect(focusFor('looks')).toBe('figure');
    expect(focusFor('body')).toBe('figure');
    expect(focusFor('face')).toBe('head');
    expect(focusFor('hair')).toBe('hair');
    expect(focusFor('hats')).toBe('hat');
    expect(focusFor('tops')).toBe('torso');
    expect(focusFor('bottoms')).toBe('legs');
    expect(focusFor('shoes')).toBe('feet');
  });

  it('accessories start on the upper body and follow the piece picked there', () => {
    expect(focusFor('accessories')).toBe('upper');
    expect(focusFor('accessories', 'glasses')).toBe('head');
    expect(focusFor('accessories', 'neck')).toBe('upper');
    expect(focusFor('accessories', 'back')).toBe('back');
    expect(focusFor('accessories', 'hands')).toBe('torso');
    expect(focusFor('accessories', 'belt')).toBe('torso');
  });

  it('anything else is the whole figure', () => {
    expect(focusFor('nonsense')).toBe('figure');
    expect(focusFor('tops', 'glasses')).toBe('torso');
  });
});

describe('shotOf', () => {
  it('the whole figure keeps the standing view it always had', () => {
    expect(shotOf('figure', 'still')).toEqual({ target: [0, 13, 0], height: 50, width: 30, yaw: -0.55, pitch: 0.14 });
  });

  it('a close-up aims at its part and is tighter than the whole figure', () => {
    const figure = shotOf('figure', 'still');
    const head = shotOf('head', 'still');
    const torso = shotOf('torso', 'still');
    const legs = shotOf('legs', 'still');
    const feet = shotOf('feet', 'still');
    // The head is texels 24 to 32 up; head and shoulders fit.
    expect(head.target[1]).toBeGreaterThan(24);
    expect(head.target[1]).toBeLessThan(30);
    expect(head.target[1] - head.height / 2).toBeLessThan(22);
    expect(head.target[1] + head.height / 2).toBeGreaterThan(33);
    expect(torso.target[1]).toBeGreaterThan(12);
    expect(torso.target[1]).toBeLessThan(24);
    // Arms included: the torso is 8 wide and each arm 4.
    expect(torso.width).toBeGreaterThanOrEqual(20);
    expect(legs.target[1]).toBeGreaterThan(0);
    expect(legs.target[1]).toBeLessThan(12);
    expect(feet.target[1]).toBeLessThan(5);
    for (const shot of [head, torso, legs, feet]) expect(fitDistance(shot, FIT)).toBeLessThan(fitDistance(figure, FIT) * 0.75);
  });

  it('hair is seen from three quarters and a little above, the face nearer the front', () => {
    const face = shotOf('head', 'still');
    const hair = shotOf('hair', 'still');
    expect(Math.abs(hair.yaw)).toBeGreaterThan(Math.abs(face.yaw) + 0.3);
    expect(hair.pitch).toBeGreaterThan(face.pitch + 0.15);
    expect(Math.abs(face.yaw)).toBeLessThan(0.45);
  });

  it('the back is seen from behind', () => {
    expect(Math.cos(shotOf('back', 'still').yaw)).toBeLessThan(-0.5);
  });

  it('follows the figure where a pose puts it', () => {
    const standing = shotOf('head', 'still');
    // Sitting on the floor, the whole figure is 9.5 lower.
    expect(shotOf('head', 'sit').target[1]).toBeCloseTo(standing.target[1] - 9.5);
    // On the horse it is up on the saddle.
    expect(shotOf('head', 'horse').target[1]).toBeGreaterThan(standing.target[1] + 10);
    // Crouching leans the head forward and down.
    const crouch = shotOf('head', 'crouch');
    expect(crouch.target[2]).toBeGreaterThan(3);
    expect(crouch.target[1]).toBeLessThan(standing.target[1] - 2);
    // Sitting, the legs lie out in front: the feet are forward at hip height.
    const feet = shotOf('feet', 'sit');
    expect(feet.target[2]).toBeGreaterThan(8);
    expect(feet.target[1]).toBeCloseTo(12 - 9.5, 0);
  });

  it('in the boat and on the horse the legs are out of sight: their close-ups are the whole figure', () => {
    for (const pose of ['boat', 'horse'] as const) {
      expect(shotOf('legs', pose)).toEqual(shotOf('figure', pose));
      expect(shotOf('feet', pose)).toEqual(shotOf('figure', pose));
      expect(shotOf('head', pose)).not.toEqual(shotOf('figure', pose));
    }
  });

  it("the launcher's own figure stands in a tighter frame that still holds it whole, a raised hand included", () => {
    const figure = shotOf('figure', 'still');
    for (const pose of ['still', 'wave'] as const) {
      const stand = shotOf('stand', pose);
      expect(stand.height).toBeLessThan(figure.height);
      // Feet on 0, crown at 32, a waving hand a little above it and out past the shoulder.
      expect(stand.target[1] - stand.height / 2).toBeLessThan(0);
      expect(stand.target[1] + stand.height / 2).toBeGreaterThan(35);
      expect(stand.width / 2).toBeGreaterThanOrEqual(14);
    }
  });

  it('every focus has a sane shot in every pose', () => {
    for (const pose of POSES) {
      for (const focus of FOCUSES) {
        const shot = shotOf(focus, pose);
        expect(shot.height).toBeGreaterThan(8);
        expect(shot.width).toBeGreaterThan(8);
        expect(shot.target.every(Number.isFinite)).toBe(true);
        expect(Math.abs(shot.pitch)).toBeLessThanOrEqual(1.2);
      }
    }
  });
});

describe('cameraAt', () => {
  const figure = shotOf('figure', 'still');

  it('stands back far enough for the frame to fit, tall or wide', () => {
    expect(fitDistance({ height: 50, width: 30 }, FIT)).toBeCloseTo(25 / FIT.half);
    // A narrow stage: the width decides.
    expect(fitDistance({ height: 50, width: 30 }, { half: FIT.half, aspect: 0.5 })).toBeCloseTo(15 / (FIT.half * 0.5));
  });

  it('looks at the target from the shot’s angle', () => {
    const cam = cameraAt(ahead(1), figure, figure, FIT);
    expect(cam.target).toEqual([0, 13, 0]);
    expect(cam.position[0]).toBeCloseTo(0);
    expect(cam.position[1]).toBeCloseTo(13);
    expect(cam.position[2]).toBeCloseTo(fitDistance(figure, FIT));
  });

  it('zooming out of a close-up recentres on the whole figure', () => {
    const head = shotOf('head', 'still');
    const near = cameraAt(ahead(1), head, figure, FIT);
    expect(near.target).toEqual(head.target);
    const out = fitDistance(figure, FIT) / fitDistance(head, FIT);
    const far = cameraAt(ahead(out), head, figure, FIT);
    expect(far.target[1]).toBeCloseTo(13);
    const half = cameraAt(ahead((1 + out) / 2), head, figure, FIT);
    expect(half.target[1]).toBeGreaterThan(13);
    expect(half.target[1]).toBeLessThan(head.target[1]);
    // Zooming in stays on the part.
    expect(cameraAt(ahead(0.7), head, figure, FIT).target).toEqual(head.target);
  });

  it('zoom reaches the whole figure from a close-up, and no further out or in than the stage itself allows', () => {
    const head = shotOf('head', 'still');
    const dFigure = fitDistance(figure, FIT);
    const dHead = fitDistance(head, FIT);
    expect(clampZoom(100, head, figure, FIT) * dHead).toBeCloseTo(ZOOM_MAX * dFigure);
    expect(clampZoom(100, figure, figure, FIT)).toBeCloseTo(ZOOM_MAX);
    // In as close as the stage's own zoom comes: a few texels fill it.
    expect(clampZoom(0.001, figure, figure, FIT)).toBeCloseTo(ZOOM_MIN);
    // A close-up comes as close as the whole figure can be, never less.
    expect(clampZoom(0.001, head, figure, FIT)).toBeLessThan(0.8);
    expect(clampZoom(0.001, head, figure, FIT) * dHead).toBeLessThanOrEqual(ZOOM_MIN * dFigure + 1e-9);
    expect(clampZoom(1, head, figure, FIT)).toBe(1);
  });

  it('a slide moves what the camera looks at and the camera with it, off the shot’s centre', () => {
    const head = shotOf('head', 'still');
    const still = cameraAt(ahead(1), head, figure, FIT);
    const slid = cameraAt({ ...ahead(1), pan: [2, -3, 0] }, head, figure, FIT);
    expect(slid.target).toEqual([head.target[0] + 2, head.target[1] - 3, head.target[2]]);
    for (const i of [0, 1, 2]) expect(slid.position[i]! - still.position[i]!).toBeCloseTo(slid.target[i]! - still.target[i]!);
  });
});

describe('zoomShot', () => {
  const figure = shotOf('figure', 'still');

  /** Where `point` lands on the screen for a view of `shot`: its offsets along the camera's right and up over its depth. */
  function onScreen(view: View, shot: Frame, point: Vec3): [number, number] {
    const eye = cameraAt(view, shot, figure, FIT).position;
    const d: Vec3 = [point[0] - eye[0], point[1] - eye[1], point[2] - eye[2]];
    const { right, up } = screenAxes(view);
    const back = backward(view);
    const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    const depth = -dot(d, back);
    return [dot(d, right) / depth, dot(d, up) / depth];
  }

  it('on the whole figure it is the stage’s own zoom toward the pointer', () => {
    const view = startView();
    const hand: Vec3 = [-6, 11, 2];
    const closer = zoomShot(view, 0.5, figure, figure, FIT, hand);
    expect(closer.zoom).toBe(0.5);
    const [x, y] = onScreen(closer, figure, hand);
    const [x0, y0] = onScreen(view, figure, hand);
    expect(x).toBeCloseTo(x0, 6);
    expect(y).toBeCloseTo(y0, 6);
  });

  it('keeps the point under the pointer on a close-up too, in and out, while the shot’s centre slides to the figure', () => {
    const head = shotOf('head', 'still');
    const view: View = { ...startView(), yaw: head.yaw, pitch: head.pitch };
    const ear: Vec3 = [4, 28, 0];
    const before = onScreen(view, head, ear);
    for (const factor of [0.6, 1.4, 2]) {
      const next = zoomShot(view, factor, head, figure, FIT, ear);
      expect(next.zoom).toBeCloseTo(factor);
      const after = onScreen(next, head, ear);
      expect(after[0]).toBeCloseTo(before[0], 6);
      expect(after[1]).toBeCloseTo(before[1], 6);
    }
    // Zoomed out with no point, the camera closes on the centre as it slides toward the whole figure's.
    const out = zoomShot(view, 2, head, figure, FIT, null);
    expect(out.pan).toEqual([0, 0, 0]);
    expect(centreAt(out.zoom, head, figure, FIT)[1]).toBeLessThan(head.target[1]);
  });

  it('stays within the shot’s limits, and a zoom held at its limit leaves the view where it is', () => {
    const head = shotOf('head', 'still');
    const view = startView();
    const far = zoomShot(view, 1000, head, figure, FIT, [1, 2, 3]);
    expect(far.zoom * fitDistance(head, FIT)).toBeCloseTo(ZOOM_MAX * fitDistance(figure, FIT));
    expect(zoomShot(far, 2, head, figure, FIT, [5, 20, 3])).toEqual(far);
    const near = zoomShot(view, 0.0001, head, figure, FIT, null);
    expect(near.zoom).toBe(clampZoom(0, head, figure, FIT));
  });
});

describe('glideView', () => {
  it('a reset glides the slide away with the angle and the zoom', () => {
    const from: View = { yaw: 1, pitch: 0.5, zoom: 0.4, pan: [6, -4, 2] };
    const to = startView();
    expect(glideView(from, to, 0)).toEqual(from);
    expect(glideView(from, to, 1)).toEqual(to);
    const mid = glideView(from, to, 0.5);
    expect(mid.pan).toEqual([3, -2, 1]);
    expect(mid.zoom).toBeCloseTo(0.7);
  });
});

describe('glideShot', () => {
  const from: Shot = { target: [0, 13, 0], height: 50, width: 30, yaw: -0.55, pitch: 0.14 };
  const to: Shot = { target: [0, 27, 0], height: 20, width: 20, yaw: -0.3, pitch: 0.06 };

  it('eases out: fast first, settled at the end', () => {
    expect(easeOut(0)).toBe(0);
    expect(easeOut(1)).toBe(1);
    expect(easeOut(0.5)).toBeGreaterThan(0.8);
    expect(GLIDE_MS).toBe(350);
  });

  it('starts at one shot and ends on the other', () => {
    expect(glideShot(from, to, 0)).toEqual(from);
    expect(glideShot(from, to, 1)).toEqual(to);
    const mid = glideShot(from, to, 0.5);
    expect(mid.target[1]).toBeCloseTo(20);
    expect(mid.height).toBeCloseTo(35);
  });

  it('turns the short way round, however many turns the player made', () => {
    const spun = { ...from, yaw: -0.55 + Math.PI * 4 + 0.2 };
    const mid = glideShot(spun, { ...from, yaw: -0.55 }, 0.5);
    expect(mid.yaw).toBeCloseTo(spun.yaw - 0.1);
    // The end is the same direction as the goal, not four turns back.
    const end = glideShot(spun, { ...from, yaw: -0.55 }, 1);
    expect(Math.cos(end.yaw)).toBeCloseTo(Math.cos(-0.55));
    expect(Math.abs(end.yaw - spun.yaw)).toBeLessThan(Math.PI);
  });
});

describe('following', () => {
  it('a new category moves the camera', () => {
    const start = following();
    const tops = start.focus('torso', 'tops');
    expect(tops.move).toBe(true);
    expect(tops.state.aimed).toBe('torso');
  });

  it('once the player has moved the camera it stays put until the category changes', () => {
    let f = following().focus('upper', 'accessories').state;
    f = f.moved();
    // A piece picked in the same category: the camera is the player's.
    const picked = f.focus('head', 'accessories');
    expect(picked.move).toBe(false);
    expect(picked.state.aimed).toBe('upper');
    // Another category: it moves again.
    const hats = picked.state.focus('hat', 'hats');
    expect(hats.move).toBe(true);
    expect(hats.state.aimed).toBe('hat');
  });

  it('untouched, it follows the piece picked within a category', () => {
    const f = following().focus('upper', 'accessories').state;
    const glasses = f.focus('head', 'accessories');
    expect(glasses.move).toBe(true);
    expect(glasses.state.aimed).toBe('head');
    // The same focus again is nothing new.
    expect(glasses.state.focus('head', 'accessories').move).toBe(false);
  });

  it('reset view goes to the framing wanted now and follows again', () => {
    let f = following().focus('upper', 'accessories').state.moved();
    f = f.focus('back', 'accessories').state;
    const reset = f.reset();
    expect(reset.aimed).toBe('back');
    expect(reset.focus('head', 'accessories').move).toBe(true);
  });
});
