import { fireEvent, render, screen } from '@testing-library/react';
import { createRef } from 'react';
import { beforeEach, expect, it, vi } from 'vitest';
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
