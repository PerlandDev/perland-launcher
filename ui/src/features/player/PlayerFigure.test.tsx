import { screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { starterSkin } from '../skins/starter';
import { renderApp } from '../../test/backend';
import { PlayerFigure } from './PlayerFigure';

// jsdom has no WebGL and no canvas: a stand-in scene records what the figure asks of it, and the codec decodes every
// texture to one marked texel.
const scene = vi.hoisted(() => ({
  setTexture: vi.fn(),
  setModel: vi.fn(),
  setPose: vi.fn(),
  setRunning: vi.fn(),
  focus: vi.fn(),
  resize: vi.fn(),
  dispose: vi.fn(),
}));
const made = vi.hoisted(() => ({ create: vi.fn() }));
vi.mock('../skins/stageScene', () => ({ createStage: made.create }));
vi.mock('../skins/codec', () => ({
  decodeTexture: async () => {
    const data = new Uint8ClampedArray(64 * 64 * 4);
    data[0] = 77;
    return data;
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
  made.create.mockImplementation(() => scene);
  // jsdom has no WebGL; say there is one, so the figure asks for its scene (the stand-in above).
  vi.stubGlobal('WebGLRenderingContext', class {});
});
afterEach(() => {
  vi.unstubAllGlobals();
});

it("shows the active account's skin in 3D, framed standing, at half the editor's frame rate", async () => {
  renderApp(<PlayerFigure />);
  await vi.waitFor(() => expect(scene.setTexture).toHaveBeenCalled());
  expect(made.create).toHaveBeenCalledWith(expect.any(HTMLCanvasElement), { maxFps: 30 });
  expect(scene.focus).toHaveBeenCalledWith('stand', 'player');
  expect((scene.setTexture.mock.lastCall![0] as Uint8ClampedArray)[0]).toBe(77);
  // Steve's first wardrobe skin has slim arms.
  expect(scene.setModel).toHaveBeenLastCalledWith('slim');
  expect(scene.setPose).toHaveBeenLastCalledWith('still');
  expect(scene.setRunning).toHaveBeenLastCalledWith(true);
  expect(screen.getByTestId('player-figure')).toHaveAttribute('data-deep');
});

it("stands the launcher's starter in for an account with nothing of its own", async () => {
  renderApp(<PlayerFigure />, { skins: 'empty' });
  await vi.waitFor(() => expect(scene.setTexture).toHaveBeenCalled());
  expect(Array.from(scene.setTexture.mock.lastCall![0] as Uint8ClampedArray)).toEqual(Array.from(starterSkin().data));
});

it('waves when asked', async () => {
  renderApp(<PlayerFigure wave />);
  await vi.waitFor(() => expect(scene.setPose).toHaveBeenLastCalledWith('wave'));
  expect(scene.setRunning).toHaveBeenLastCalledWith(true);
});

it('holds still when asked: a game is running, and the figure stops redrawing', async () => {
  renderApp(<PlayerFigure still wave />);
  await vi.waitFor(() => expect(scene.setRunning).toHaveBeenLastCalledWith(false));
  // Held, it doesn't start a wave it couldn't play.
  expect(scene.setPose).toHaveBeenLastCalledWith('still');
});

it('keeps the flat figure where there is no WebGL', async () => {
  made.create.mockImplementation(() => {
    throw new Error('no WebGL');
  });
  renderApp(<PlayerFigure />);
  await vi.waitFor(() => expect(made.create).toHaveBeenCalled());
  expect(screen.getByTestId('player-figure')).not.toHaveAttribute('data-deep');
});

it("never loads the 3D figure's code in a webview with no WebGL at all", async () => {
  vi.unstubAllGlobals();
  renderApp(<PlayerFigure />);
  await new Promise((done) => setTimeout(done, 300));
  expect(made.create).not.toHaveBeenCalled();
  expect(screen.getByTestId('player-figure')).not.toHaveAttribute('data-deep');
});

it('lets go of the scene when it leaves', async () => {
  const { unmount } = renderApp(<PlayerFigure />);
  await vi.waitFor(() => expect(scene.setTexture).toHaveBeenCalled());
  unmount();
  expect(scene.dispose).toHaveBeenCalled();
});
