import { act, fireEvent, render, screen } from '@testing-library/react';
import { createRef } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { BLINK_MS, blinkGap, blinkSeed } from './blink';
import { Pixels } from './paint';
import { Stage, type StageHandle } from './Stage';

// The scene without WebGL: it records what the stage asks of it.
const scene = vi.hoisted(() => ({
  calls: [] as [string, ...unknown[]][],
  hit: null as { x: number; y: number } | null,
}));
vi.mock('./stageScene', () => ({
  createStage: () => {
    const log = (name: string) => (...args: unknown[]) => void scene.calls.push([name, ...args]);
    return {
      setTexture: (data: Uint8ClampedArray) => void scene.calls.push(['setTexture', new Uint8ClampedArray(data)]),
      setModel: log('setModel'),
      setVisible: () => {},
      setPaintLayer: log('setPaintLayer'),
      setPose: log('setPose'),
      setPaused: log('setPaused'),
      celebrate: log('celebrate'),
      pick: () => scene.hit,
      focus: log('focus'),
      orbit: log('orbit'),
      zoom: log('zoom'),
      resetView: log('resetView'),
      resize: () => {},
      dispose: log('dispose'),
    };
  },
}));

beforeEach(() => {
  scene.calls = [];
  scene.hit = null;
});
afterEach(() => {
  vi.useRealTimers();
});

const noop = () => {};
const base = { pixels: new Pixels(), rev: 0, model: 'classic' as const, hidden: new Set<string>(), paintLayer: 'base' as const, pose: 'still' as const, paused: false, hover: null, onHover: noop, onBegin: noop, onPaint: noop, onEnd: noop };
const called = (name: string) => scene.calls.filter(([n]) => n === name).map(([, ...args]) => args);

it('frames the whole figure unless told what is being dressed, and passes each new focus on', () => {
  const { rerender } = render(<Stage {...base} />);
  expect(called('focus')).toEqual([['figure', '']]);
  rerender(<Stage {...base} focus="head" focusKey="create:face" />);
  rerender(<Stage {...base} focus="head" focusKey="create:face" rev={1} />);
  rerender(<Stage {...base} focus="hat" focusKey="create:hats" />);
  expect(called('focus')).toEqual([['figure', ''], ['head', 'create:face'], ['hat', 'create:hats']]);
});

it('turning, zooming and resetting the view go straight to the scene', () => {
  const handle = createRef<StageHandle>();
  render(<Stage {...base} paintable={false} ref={handle} />);
  const canvas = screen.getByTestId('stage');
  fireEvent.pointerDown(canvas, { clientX: 10, clientY: 10, button: 0, pointerId: 1 });
  fireEvent.pointerMove(canvas, { clientX: 30, clientY: 15, pointerId: 1 });
  fireEvent.pointerUp(canvas, { pointerId: 1 });
  expect(called('orbit')).toEqual([[20, 5]]);
  fireEvent.wheel(canvas, { deltaY: 100 });
  expect(called('zoom')).toHaveLength(1);
  handle.current!.resetView();
  expect(called('resetView')).toHaveLength(1);
});

// The blink: the figure's texture is swapped for the closed-eyes frame for a moment, and nothing else is.
const SKIN = 'skin-0a1b2c3d4e5f';
const OPEN = new Pixels();
OPEN.data.fill(200);
const SHUT = new Pixels();
SHUT.data.fill(90);
const worn = () => (called('setTexture').at(-1)![0] as Uint8ClampedArray)[0];

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

it('does not blink while paused, or with no closed-eyes frame; a frame taken away mid-blink opens the eyes', () => {
  vi.useFakeTimers();
  const { rerender } = render(<Stage {...base} pixels={OPEN} blink={SHUT} blinkSeed={SKIN} paused />);
  act(() => void vi.advanceTimersByTime(30_000));
  expect(called('setTexture').every(([data]) => (data as Uint8ClampedArray)[0] === 200)).toBe(true);
  rerender(<Stage {...base} pixels={OPEN} blinkSeed={SKIN} />);
  act(() => void vi.advanceTimersByTime(30_000));
  expect(called('setTexture').every(([data]) => (data as Uint8ClampedArray)[0] === 200)).toBe(true);
  rerender(<Stage {...base} pixels={OPEN} blink={SHUT} blinkSeed={SKIN} />);
  act(() => void vi.advanceTimersByTime(blinkGap(blinkSeed(SKIN), 0)));
  expect(worn()).toBe(90);
  // A preview or a stroke takes the frame away.
  rerender(<Stage {...base} pixels={OPEN} blink={null} blinkSeed={SKIN} />);
  expect(worn()).toBe(200);
});
