import type { Focus } from '../ipc/schema';

export type Size = { width: number; height: number };
export type Box = { left: number; top: number; width: number; height: number };

/** Where a picture background sits when the player hasn't chosen: the bottom middle. */
export const DEFAULT_FOCUS: Focus = { x: 50, y: 100 };
/** The zoom's range and step: the picture's height in percent of the marquee's (100 is as tall as the marquee). */
export const MIN_ZOOM = 25;
export const MAX_ZOOM = 500;
export const ZOOM_STEP = 5;

/** A picture's box in the marquee, in the frame's pixels. With no zoom chosen it just covers the frame (CSS `cover`),
 *  as it always has. A chosen zoom is the picture's height in percent of the frame's: the frame's height, unlike its
 *  width, barely changes when the window does, so the picture keeps its size beside the name and Play at any window
 *  width. The focus keeps `object-position`'s meaning: x% and y% place the picture in the free space the frame
 *  leaves (or the overflow, when it is bigger). */
export function bannerBox(frame: Size, picture: Size, zoom: number | null, focus: Focus | null): Box {
  const scale =
    zoom === null ? Math.max(frame.width / picture.width, frame.height / picture.height) : ((zoom / 100) * frame.height) / picture.height;
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

/** The zoom that just covers `frame`: what the marquee shows with no zoom chosen, so adjusting starts from it. */
export function coverZoom(frame: Size, picture: Size): number {
  const cover = Math.max(frame.width / picture.width, frame.height / picture.height);
  return clampZoom(((cover * picture.height) / frame.height) * 100);
}

/** Whether `box` leaves part of `frame` uncovered, where the edges show. Half a pixel is rounding, not an edge. */
export function leavesEdges(frame: Size, box: Box): boolean {
  return box.left > 0.5 || box.top > 0.5 || box.left + box.width < frame.width - 0.5 || box.top + box.height < frame.height - 0.5;
}
