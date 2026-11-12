import { useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { usePlayerSkin } from '../../state/queries';
import { decodeTexture } from '../skins/codec';
import { starterSkin } from '../skins/starter';
import type { Stage } from '../skins/stageScene';
import { flatFront, FLAT_H, FLAT_W } from './flatFront';

const REDUCED = '(prefers-reduced-motion: reduce)';

/** The system asks for less motion. */
function useReducedMotion(): boolean {
  return useSyncExternalStore(
    (changed) => {
      const query = window.matchMedia?.(REDUCED);
      query?.addEventListener('change', changed);
      return () => query?.removeEventListener('change', changed);
    },
    () => window.matchMedia?.(REDUCED).matches ?? false,
    () => false,
  );
}

/** Once the browser is idle (or soon after, where it can't say), so loading the 3D figure never delays a first paint. */
function whenIdle(run: () => void): () => void {
  if (typeof window.requestIdleCallback === 'function') {
    const id = window.requestIdleCallback(run, { timeout: 1500 });
    return () => window.cancelIdleCallback(id);
  }
  const id = window.setTimeout(run, 150);
  return () => window.clearTimeout(id);
}

/**
 * The player's own figure, wearing the active account's skin (the launcher's starter when it has none of its own).
 *
 * It shows at once as a flat front view, drawn once. The 3D figure (the Skins stage's own, so three.js loads only after
 * the first paint and is shared with that screen) then takes its place: it breathes and looks around, and waves while
 * `wave` is set. It draws 30 frames a second at most, and none at all while `still` is set (its owner holds it) or the
 * system asks for less motion. Without WebGL the flat figure simply stays.
 *
 * It is a picture: hidden from assistive tech, and whatever it stands beside says what it is for.
 */
export function PlayerFigure({ wave = false, still = false, className }: { wave?: boolean; still?: boolean; className?: string }) {
  const skin = usePlayerSkin();
  const texture = skin.data?.texture;
  const model = skin.data?.model ?? 'classic';
  const decoded = useQuery({ queryKey: ['playerTexels', texture], queryFn: () => decodeTexture(texture ?? ''), enabled: Boolean(texture), staleTime: Infinity });
  const starter = useMemo(() => starterSkin(model).data, [model]);
  // Nothing until the account's answer is in; then its own skin, or the starter when it has none (or it can't be read).
  const texels = skin.data === undefined ? null : texture && !decoded.isError ? (decoded.data ?? null) : starter;
  const calm = useReducedMotion();
  const held = still || calm;

  const flat = useRef<HTMLCanvasElement>(null);
  const deep = useRef<HTMLCanvasElement>(null);
  const [stage, setStage] = useState<Stage | null>(null);

  useEffect(() => {
    const ctx = flat.current?.getContext('2d');
    if (!ctx || !texels) return;
    const image = ctx.createImageData(FLAT_W, FLAT_H);
    image.data.set(flatFront(texels, model));
    ctx.putImageData(image, 0, 0);
  }, [texels, model]);

  useEffect(() => {
    const el = deep.current;
    // A webview with no WebGL at all never loads the 3D figure's code.
    if (!el || typeof window.WebGLRenderingContext === 'undefined') return;
    let made: Stage | null = null;
    let gone = false;
    let observer: ResizeObserver | null = null;
    const cancel = whenIdle(() => {
      void import('../skins/stageScene')
        .then(({ createStage }) => {
          if (gone) return;
          made = createStage(el, { maxFps: 30 });
          made.focus('stand', 'player');
          const scene = made;
          const fit = () => scene.resize(el.clientWidth, el.clientHeight);
          fit();
          observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(fit);
          observer?.observe(el);
          setStage(made);
        })
        // No WebGL, or the figure's code couldn't load: the flat figure stays.
        .catch(() => undefined);
    });
    return () => {
      gone = true;
      cancel();
      observer?.disconnect();
      made?.dispose();
      setStage(null);
    };
  }, []);

  useEffect(() => stage?.setModel(model), [stage, model]);
  useEffect(() => {
    if (texels) stage?.setTexture(texels);
  }, [stage, texels, model]);
  // Held, it doesn't start a wave it couldn't play.
  useEffect(() => stage?.setPose(wave && !held ? 'wave' : 'still'), [stage, wave, held]);
  useEffect(() => stage?.setRunning(!held), [stage, held]);

  return (
    <span className={`player-figure ${className ?? ''}`} data-testid="player-figure" data-deep={stage && texels ? '' : undefined} aria-hidden>
      <canvas ref={flat} className="player-flat" width={FLAT_W} height={FLAT_H} />
      <canvas ref={deep} className="player-deep" />
    </span>
  );
}
