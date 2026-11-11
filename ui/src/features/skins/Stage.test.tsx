import { act, fireEvent, render, screen } from '@testing-library/react';
import { createRef } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { BLINK_MS, blinkGap, blinkSeed } from './blink';
import { Pixels } from './paint';
import { starterSkin } from './starter';
import { Stage, type StageHandle } from './Stage';

// jsdom has no WebGL: a stand-in scene records what the stage asks of it. The textures it is handed are copied as
// they arrive, so a test reads what the figure wore at each call.
const scene = vi.hoisted(() => ({
  setTexture: vi.fn(),
  setModel: vi.fn(),
  setVisible: vi.fn(),
  setPaintLayer: vi.fn(),
  setPose: vi.fn(),
  setStraight: vi.fn(),
  celebrate: vi.fn(),
  pick: vi.fn((): unknown => null),
  focus: vi.fn(),
  orbit: vi.fn(),
  zoom: vi.fn(),
  pan: vi.fn(),
  resetView: vi.fn(),
  resize: vi.fn(),
  dispose: vi.fn(),
  textures: [] as Uint8ClampedArray[],
}));
vi.mock('./stageScene', () => ({ createStage: () => scene }));

beforeEach(() => {
  vi.clearAllMocks();
  scene.textures = [];
  scene.setTexture.mockImplementation((data: Uint8ClampedArray) => void scene.textures.push(new Uint8ClampedArray(data)));
  scene.pick.mockReturnValue(null);
});
afterEach(() => {
  vi.useRealTimers();
});

function renderStage() {
  const onPaint = vi.fn();
  const view = render(
    <Stage pixels={starterSkin()} rev={0} model="classic" hidden={new Set()} paintLayer="base" pose="still" straight={false} hover={null} onHover={() => {}} onBegin={() => {}} onPaint={onPaint} onEnd={() => {}} />,
  );
  return { canvas: view.getByTestId('stage'), onPaint };
}

it('the straight pose reaches the scene', () => {
  const view = render(
    <Stage pixels={starterSkin()} rev={0} model="classic" hidden={new Set()} paintLayer="base" pose="walk" straight hover={null} onHover={() => {}} onBegin={() => {}} onPaint={() => {}} onEnd={() => {}} />,
  );
  expect(scene.setStraight).toHaveBeenLastCalledWith(true);
  view.rerender(
    <Stage pixels={starterSkin()} rev={0} model="classic" hidden={new Set()} paintLayer="base" pose="walk" straight={false} hover={null} onHover={() => {}} onBegin={() => {}} onPaint={() => {}} onEnd={() => {}} />,
  );
  expect(scene.setStraight).toHaveBeenLastCalledWith(false);
});

it('the wheel zooms toward the pointer', () => {
  const { canvas } = renderStage();
  fireEvent.wheel(canvas, { deltaY: -100, clientX: 40, clientY: 60 });
  expect(scene.zoom).toHaveBeenCalledWith(1 / 1.1, 40, 60);
  fireEvent.wheel(canvas, { deltaY: 100, clientX: 5, clientY: 6 });
  expect(scene.zoom).toHaveBeenLastCalledWith(1.1, 5, 6);
});

it('tilting the wheel sideways (a horizontal scroll) does not zoom', () => {
  const { canvas } = renderStage();
  fireEvent.wheel(canvas, { deltaX: 120, deltaY: 0, clientX: 40, clientY: 60 });
  fireEvent.wheel(canvas, { deltaX: -120, deltaY: 0, clientX: 40, clientY: 60 });
  // A trackpad swipe that is mostly sideways doesn't zoom either.
  fireEvent.wheel(canvas, { deltaX: 80, deltaY: 3, clientX: 40, clientY: 60 });
  expect(scene.zoom).not.toHaveBeenCalled();
  // The wheel itself still zooms.
  fireEvent.wheel(canvas, { deltaX: 0, deltaY: 100, clientX: 40, clientY: 60 });
  expect(scene.zoom).toHaveBeenCalledWith(1.1, 40, 60);
});

it('a middle-drag or a Shift-drag slides the view, even over the figure, and paints nothing', () => {
  const { canvas, onPaint } = renderStage();
  scene.pick.mockReturnValue({ x: 9, y: 9, part: 'head', layer: 'base' });
  fireEvent.pointerDown(canvas, { button: 1, clientX: 10, clientY: 10, pointerId: 1 });
  fireEvent.pointerMove(canvas, { clientX: 14, clientY: 7, pointerId: 1 });
  fireEvent.pointerUp(canvas, { pointerId: 1 });
  expect(scene.pan).toHaveBeenCalledWith(4, -3);
  fireEvent.pointerDown(canvas, { button: 0, shiftKey: true, clientX: 0, clientY: 0, pointerId: 1 });
  fireEvent.pointerMove(canvas, { clientX: 2, clientY: 5, pointerId: 1 });
  fireEvent.pointerUp(canvas, { pointerId: 1 });
  expect(scene.pan).toHaveBeenLastCalledWith(2, 5);
  expect(scene.orbit).not.toHaveBeenCalled();
  expect(onPaint).not.toHaveBeenCalled();
});

it('a plain drag on the figure still paints', () => {
  const { canvas, onPaint } = renderStage();
  scene.pick.mockReturnValue({ x: 9, y: 9, part: 'head', layer: 'base' });
  fireEvent.pointerDown(canvas, { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
  expect(onPaint).toHaveBeenCalledWith(9, 9);
  expect(scene.pan).not.toHaveBeenCalled();
});

// The Create and Catalog side: what the camera frames, a figure that isn't painted on, and the blink.
const noop = () => {};
const base = { pixels: new Pixels(), rev: 0, model: 'classic' as const, hidden: new Set<string>(), paintLayer: 'base' as const, pose: 'still' as const, straight: false, hover: null, onHover: noop, onBegin: noop, onPaint: noop, onEnd: noop };

it('frames the whole figure unless told what is being dressed, and passes each new focus on', () => {
  const { rerender } = render(<Stage {...base} />);
  expect(scene.focus.mock.calls).toEqual([['figure', '']]);
  rerender(<Stage {...base} focus="head" focusKey="create:face" />);
  rerender(<Stage {...base} focus="head" focusKey="create:face" rev={1} />);
  rerender(<Stage {...base} focus="hat" focusKey="create:hats" />);
  expect(scene.focus.mock.calls).toEqual([['figure', ''], ['head', 'create:face'], ['hat', 'create:hats']]);
});

it('turning, zooming and resetting the view go straight to the scene', () => {
  const handle = createRef<StageHandle>();
  render(<Stage {...base} paintable={false} ref={handle} />);
  const canvas = screen.getByTestId('stage');
  fireEvent.pointerDown(canvas, { clientX: 10, clientY: 10, button: 0, pointerId: 1 });
  fireEvent.pointerMove(canvas, { clientX: 30, clientY: 15, pointerId: 1 });
  fireEvent.pointerUp(canvas, { pointerId: 1 });
  expect(scene.orbit.mock.calls).toEqual([[20, 5]]);
  fireEvent.wheel(canvas, { deltaY: 100 });
  expect(scene.zoom).toHaveBeenCalledTimes(1);
  handle.current!.resetView();
  expect(scene.resetView).toHaveBeenCalledTimes(1);
});

it('a figure that is not painted on turns under a drag on it, and still slides under a middle-drag or a Shift-drag', () => {
  const onPaint = vi.fn();
  const onHover = vi.fn();
  render(<Stage {...base} paintable={false} onPaint={onPaint} onHover={onHover} />);
  const canvas = screen.getByTestId('stage');
  scene.pick.mockReturnValue({ x: 9, y: 9, part: 'head', layer: 'base' });
  fireEvent.pointerMove(canvas, { clientX: 3, clientY: 3, pointerId: 1 });
  fireEvent.pointerDown(canvas, { clientX: 10, clientY: 10, button: 0, pointerId: 1 });
  fireEvent.pointerMove(canvas, { clientX: 12, clientY: 11, pointerId: 1 });
  fireEvent.pointerUp(canvas, { pointerId: 1 });
  expect(scene.orbit.mock.calls).toEqual([[2, 1]]);
  fireEvent.pointerDown(canvas, { clientX: 0, clientY: 0, button: 1, pointerId: 1 });
  fireEvent.pointerMove(canvas, { clientX: 6, clientY: -2, pointerId: 1 });
  fireEvent.pointerUp(canvas, { pointerId: 1 });
  expect(scene.pan).toHaveBeenCalledWith(6, -2);
  expect(onPaint).not.toHaveBeenCalled();
  expect(onHover).not.toHaveBeenCalled();
});

// The blink: the figure's texture is swapped for the closed-eyes frame for a moment, and nothing else is.
const SKIN = 'skin-0a1b2c3d4e5f';
const OPEN = new Pixels();
OPEN.data.fill(200);
const SHUT = new Pixels();
SHUT.data.fill(90);
const worn = () => scene.textures.at(-1)![0];

it('blinks at the skin’s rhythm: the closed-eyes frame for a moment, then the skin again', () => {
  vi.useFakeTimers();
  render(<Stage {...base} pixels={OPEN} blink={SHUT} blinkSeed={SKIN} />);
  expect(worn()).toBe(200);
  const gap = blinkGap(blinkSeed(SKIN), 0);
  act(() => void vi.advanceTimersByTime(gap - 1));
  expect(worn()).toBe(200);
  act(() => void vi.advanceTimersByTime(1));
  expect(worn()).toBe(90);
  act(() => void vi.advanceTimersByTime(BLINK_MS));
  expect(worn()).toBe(200);
  // The frames it was given are as they were.
  expect(OPEN.data[0]).toBe(200);
  expect(SHUT.data[0]).toBe(90);
});

it('does not blink with Animation off, or with no closed-eyes frame; a frame taken away mid-blink opens the eyes', () => {
  vi.useFakeTimers();
  const { rerender } = render(<Stage {...base} pixels={OPEN} blink={SHUT} blinkSeed={SKIN} straight />);
  act(() => void vi.advanceTimersByTime(30_000));
  expect(scene.textures.every((data) => data[0] === 200)).toBe(true);
  rerender(<Stage {...base} pixels={OPEN} blinkSeed={SKIN} />);
  act(() => void vi.advanceTimersByTime(30_000));
  expect(scene.textures.every((data) => data[0] === 200)).toBe(true);
  rerender(<Stage {...base} pixels={OPEN} blink={SHUT} blinkSeed={SKIN} />);
  act(() => void vi.advanceTimersByTime(blinkGap(blinkSeed(SKIN), 0)));
  expect(worn()).toBe(90);
  // A preview or a stroke takes the frame away.
  rerender(<Stage {...base} pixels={OPEN} blink={null} blinkSeed={SKIN} />);
  expect(worn()).toBe(200);
});

it('turning Animation off mid-blink opens the eyes at once', () => {
  vi.useFakeTimers();
  const { rerender } = render(<Stage {...base} pixels={OPEN} blink={SHUT} blinkSeed={SKIN} />);
  act(() => void vi.advanceTimersByTime(blinkGap(blinkSeed(SKIN), 0)));
  expect(worn()).toBe(90);
  rerender(<Stage {...base} pixels={OPEN} blink={SHUT} blinkSeed={SKIN} straight />);
  expect(worn()).toBe(200);
});
