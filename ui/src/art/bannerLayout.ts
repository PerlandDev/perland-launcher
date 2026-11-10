import type { Focus } from '../ipc/schema';

export type Size = { width: number; height: number };
export type Box = { left: number; top: number; width: number; height: number };

/** Where a picture background sits when the player hasn't chosen: the bottom middle. */
export const DEFAULT_FOCUS: Focus = { x: 50, y: 100 };
/** The zoom's range and step: the picture's height in percent of the marquee's (100 is as tall as the marquee). */
export const MIN_ZOOM = 25;
export const MAX_ZOOM = 500;
export const ZOOM_STEP = 5;
/** The shape of the band a zoomed picture is placed across, centred in the marquee, for a view saved before the
 *  marquee's own shape was kept with it. */
export const PLACE_ASPECT = 3 / 2;

/** A marquee's shape as a view keeps it: its width per 1000 of its height. */
export function aspectOf(frame: Size): number {
  return Math.round((frame.width / frame.height) * 1000);
}

/** The band a zoomed picture is sized against and placed across: the shape the view was chosen at (`aspect`, width
 *  over height; 3:2 when not kept), as big as fits in `frame`, centred across it. A wider frame only adds room on both
 *  sides, so the picture keeps its size and its distance from the middle, beside the name and Play. A narrower frame
 *  makes the band, and so the picture, smaller, so what was framed from side to side still shows instead of being
 *  cropped: the window never zooms the picture in. */
export function placeFrame(frame: Size, aspect: number = PLACE_ASPECT): { left: number; width: number; height: number } {
  const width = Math.min(frame.width, frame.height * aspect);
  return { left: (frame.width - width) / 2, width, height: width / aspect };
}

/** A picture's box in the marquee, in the frame's pixels. With no zoom chosen it is as wide as the frame: in a frame
 *  wider than the picture that covers it, as it always has, and a narrower window shrinks it instead of cropping its
 *  sides, with the edges above and below it; x% and y% place it as `object-position` does. A chosen zoom is the picture's height in percent
 *  of `placeFrame`'s band, the shape the view was chosen at (`aspect`) fitted in the frame: in that shape, the frame's
 *  own height. Its y% places it in the frame's height, and its x% across the band, so whatever the window's shape, the
 *  part of the picture framed when it was chosen stays in view whole, with more of the picture (or the edges) around. */
export function bannerBox(frame: Size, picture: Size, zoom: number | null, focus: Focus | null, aspect?: number): Box {
  const band = placeFrame(frame, aspect);
  const scale =
    zoom === null ? frame.width / picture.width : ((zoom / 100) * band.height) / picture.height;
  const width = picture.width * scale;
  const height = picture.height * scale;
  const { x, y } = focus ?? DEFAULT_FOCUS;
  const across = zoom === null ? { left: 0, width: frame.width } : band;
  return { left: across.left + ((across.width - width) * x) / 100, top: ((frame.height - height) * y) / 100, width, height };
}

/** The position that draws a zoomed picture at `box` (as near as 0–100 allows); no free space keeps it in the middle. */
function focusAt(frame: Size, box: Box, aspect?: number): Focus {
  const band = placeFrame(frame, aspect);
  const percent = (start: number, free: number) => (Math.abs(free) < 0.5 ? 50 : Math.min(100, Math.max(0, Math.round((start / free) * 100))));
  return { x: percent(box.left - band.left, band.width - box.width), y: percent(box.top, frame.height - box.height) };
}

/** The position for `next` that keeps the part of the picture under `anchor` (a frame point; the middle when there is
 *  no pointer) where it is, so a zoom closes in on what the player is looking at instead of on a corner. */
export function zoomAround(
  frame: Size,
  picture: Size,
  zoom: number | null,
  focus: Focus | null,
  next: number,
  anchor?: { x: number; y: number },
  aspect?: number,
): Focus {
  const at = anchor ?? { x: frame.width / 2, y: frame.height / 2 };
  const box = bannerBox(frame, picture, zoom, focus, aspect);
  const height = (next / 100) * placeFrame(frame, aspect).height;
  const width = (height * picture.width) / picture.height;
  const on = { x: (at.x - box.left) / box.width, y: (at.y - box.top) / box.height };
  return focusAt(frame, { left: at.x - on.x * width, top: at.y - on.y * height, width, height }, aspect);
}

/** A zoomed view chosen at shape `from` (3:2 when not kept) restated for shape `to`: the picture stays centred where
 *  it is in `frame`, on the zoom's step (so within 2.5 % of its size). "Adjust the background" restates a view for the
 *  header it shows, so the next change keeps the header's shape with it without moving the picture first. */
export function restateView(frame: Size, picture: Size, zoom: number, focus: Focus | null, from: number | undefined, to: number): { zoom: number; focus: Focus } {
  const box = bannerBox(frame, picture, zoom, focus, from);
  const next = clampZoom((box.height / placeFrame(frame, to).height) * 100);
  const height = (next / 100) * placeFrame(frame, to).height;
  const width = (height * picture.width) / picture.height;
  const centre = { x: box.left + box.width / 2, y: box.top + box.height / 2 };
  return { zoom: next, focus: focusAt(frame, { left: centre.x - width / 2, top: centre.y - height / 2, width, height }, to) };
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

/** The zoom of the picture as wide as `frame`, against the band of shape `aspect`: what the marquee shows with no zoom
 *  chosen, so adjusting starts from it. */
export function coverZoom(frame: Size, picture: Size, aspect?: number): number {
  const scale = frame.width / picture.width;
  return clampZoom(((scale * picture.height) / placeFrame(frame, aspect).height) * 100);
}

/** Whether `box` leaves part of `frame` uncovered, where the edges show. Half a pixel is rounding, not an edge. */
export function leavesEdges(frame: Size, box: Box): boolean {
  return box.left > 0.5 || box.top > 0.5 || box.left + box.width < frame.width - 0.5 || box.top + box.height < frame.height - 0.5;
}
