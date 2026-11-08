import { fireEvent, render } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { starterSkin } from './starter';
import { Stage } from './Stage';

// jsdom has no WebGL: a stand-in scene records what the stage asks of it.
const scene = {
  setTexture: vi.fn(),
  setModel: vi.fn(),
  setVisible: vi.fn(),
  setPaintLayer: vi.fn(),
  setPose: vi.fn(),
  setPaused: vi.fn(),
  celebrate: vi.fn(),
  pick: vi.fn(() => null),
  orbit: vi.fn(),
  zoom: vi.fn(),
  pan: vi.fn(),
  resetView: vi.fn(),
  resize: vi.fn(),
  dispose: vi.fn(),
};
vi.mock('./stageScene', () => ({ createStage: () => scene }));

beforeEach(() => vi.clearAllMocks());

function renderStage() {
  const onPaint = vi.fn();
  const view = render(
    <Stage pixels={starterSkin()} rev={0} model="classic" hidden={new Set()} paintLayer="base" pose="still" paused={false} hover={null} onHover={() => {}} onBegin={() => {}} onPaint={onPaint} onEnd={() => {}} />,
  );
  return { canvas: view.getByTestId('stage'), onPaint };
}

it('the wheel zooms toward the pointer', () => {
  const { canvas } = renderStage();
  fireEvent.wheel(canvas, { deltaY: -100, clientX: 40, clientY: 60 });
  expect(scene.zoom).toHaveBeenCalledWith(1 / 1.1, 40, 60);
  fireEvent.wheel(canvas, { deltaY: 100, clientX: 5, clientY: 6 });
  expect(scene.zoom).toHaveBeenLastCalledWith(1.1, 5, 6);
});

it('a middle-drag or a Shift-drag slides the view, even over the figure, and paints nothing', () => {
  const { canvas, onPaint } = renderStage();
  scene.pick.mockReturnValue({ x: 9, y: 9, part: 'head', layer: 'base' } as never);
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
  scene.pick.mockReturnValue({ x: 9, y: 9, part: 'head', layer: 'base' } as never);
  fireEvent.pointerDown(canvas, { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
  expect(onPaint).toHaveBeenCalledWith(9, 9);
  expect(scene.pan).not.toHaveBeenCalled();
});
