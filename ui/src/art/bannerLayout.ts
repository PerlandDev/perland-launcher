import type { Focus } from '../ipc/schema';

export type Size = { width: number; height: number };
export type Box = { left: number; top: number; width: number; height: number };

/** Where a picture background sits when the player hasn't chosen: the bottom middle. */
export const DEFAULT_FOCUS: Focus = { x: 50, y: 100 };
/** The zoom's range and step, in percent of the size that just covers the marquee (100). */
export const MIN_ZOOM = 25;
export const MAX_ZOOM = 300;
export const ZOOM_STEP = 5;

/** A picture's box in the marquee, in the frame's pixels. At 100 it just covers the frame (CSS `cover`); the zoom
 *  scales it from there. The focus keeps `object-position`'s meaning on both sides of 100: x% and y% place the
 *  picture in the free space the frame leaves (or the overflow, when it is bigger). */
export function bannerBox(frame: Size, picture: Size, zoom: number | null, focus: Focus | null): Box {
  const cover = Math.max(frame.width / picture.width, frame.height / picture.height);
  const scale = cover * ((zoom ?? 100) / 100);
  const width = picture.width * scale;
  const height = picture.height * scale;
  const { x, y } = focus ?? DEFAULT_FOCUS;
  return { left: ((frame.width - width) * x) / 100, top: ((frame.height - height) * y) / 100, width, height };
}

/** A zoom the core accepts: on the step, inside the range. */
export function clampZoom(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.round(zoom / ZOOM_STEP) * ZOOM_STEP));
}

/** The focus after dragging the picture by `delta` frame pixels from `start`: the picture follows the pointer. An
 *  axis with no free space (the picture exactly fills it) can't move, so it keeps its value. */
export function dragFocus(start: Focus, delta: Size, frame: Size, box: Box): Focus {
  const along = (value: number, moved: number, free: number) => {
    if (Math.abs(free) < 0.5) return value;
    return Math.min(100, Math.max(0, Math.round(value + (moved / free) * 100)));
  };
  return { x: along(start.x, delta.width, frame.width - box.width), y: along(start.y, delta.height, frame.height - box.height) };
}

/** Whether a picture at this zoom still covers the whole frame (no edges show). */
export function covers(zoom: number | null): boolean {
  return (zoom ?? 100) >= 100;
}
