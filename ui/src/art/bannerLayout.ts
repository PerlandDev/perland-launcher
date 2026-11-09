import type { Focus } from '../ipc/schema';

export type Size = { width: number; height: number };
export type Box = { left: number; top: number; width: number; height: number };

/** Where a picture background sits when the player hasn't chosen: the bottom middle. */
export const DEFAULT_FOCUS: Focus = { x: 50, y: 100 };
/** The zoom's range and step: the picture's height in percent of the marquee's (100 is as tall as the marquee). */
export const MIN_ZOOM = 25;
export const MAX_ZOOM = 500;
export const ZOOM_STEP = 5;
/** The shape of the band a zoomed picture is placed across, centred in the marquee. */
export const PLACE_ASPECT = 3 / 2;

/** The band, as tall as `frame` and 3:2, centred across it, that a zoomed picture's x% places it in. A wider window
 *  only adds room on both sides of it, so the picture keeps its distance from the middle, beside the name and Play. */
export function placeFrame(frame: Size): { left: number; width: number } {
  const width = frame.height * PLACE_ASPECT;
  return { left: (frame.width - width) / 2, width };
}

/** A picture's box in the marquee, in the frame's pixels. With no zoom chosen it just covers the frame (CSS `cover`),
 *  as it always has, and x% and y% place it as `object-position` does. A chosen zoom is the picture's height in percent
 *  of the frame's: the frame's height, unlike its width, barely changes when the window does, so the picture keeps its
 *  size beside the name and Play. Its y% places it in the frame's height, and its x% across `placeFrame`'s band, so it
 *  keeps its place too: a wider window only adds room on both sides. */
export function bannerBox(frame: Size, picture: Size, zoom: number | null, focus: Focus | null): Box {
  const scale =
    zoom === null ? Math.max(frame.width / picture.width, frame.height / picture.height) : ((zoom / 100) * frame.height) / picture.height;
  const width = picture.width * scale;
  const height = picture.height * scale;
  const { x, y } = focus ?? DEFAULT_FOCUS;
  const across = zoom === null ? { left: 0, width: frame.width } : placeFrame(frame);
  return { left: across.left + ((across.width - width) * x) / 100, top: ((frame.height - height) * y) / 100, width, height };
}

/** The position that draws the picture in `box`, at `zoom`, centred where `box` is: choosing a zoom where none was
 *  then starts from what the marquee shows, without the picture jumping. */
export function focusFor(frame: Size, box: Box, zoom: number): Focus {
  const height = (zoom / 100) * frame.height;
  const width = (height * box.width) / box.height;
  return focusAt(frame, { left: box.left + box.width / 2 - width / 2, top: box.top + box.height / 2 - height / 2, width, height });
}

/** The position that draws a zoomed picture at `box` (as near as 0–100 allows); no free space keeps it in the middle. */
function focusAt(frame: Size, box: Box): Focus {
  const band = placeFrame(frame);
  const percent = (start: number, free: number) => (Math.abs(free) < 0.5 ? 50 : Math.min(100, Math.max(0, Math.round((start / free) * 100))));
  return { x: percent(box.left - band.left, band.width - box.width), y: percent(box.top, frame.height - box.height) };
}

/** The position for `next` that keeps the part of the picture under `anchor` (a frame point; the middle when there is
 *  no pointer) where it is, so a zoom closes in on what the player is looking at instead of on a corner. */
export function zoomAround(frame: Size, picture: Size, zoom: number | null, focus: Focus | null, next: number, anchor?: { x: number; y: number }): Focus {
  const at = anchor ?? { x: frame.width / 2, y: frame.height / 2 };
  const box = bannerBox(frame, picture, zoom, focus);
  const height = (next / 100) * frame.height;
  const width = (height * picture.width) / picture.height;
  const on = { x: (at.x - box.left) / box.width, y: (at.y - box.top) / box.height };
  return focusAt(frame, { left: at.x - on.x * width, top: at.y - on.y * height, width, height });
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
