import { useEffect, useImperativeHandle, useRef, useState, type PointerEvent, type Ref, type WheelEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type { SkinModel } from '../../ipc/schema';
import { SKIN_SIZE, type Layer, type Part } from './layout';
import type { Pixels } from './paint';
import type { Pose } from './poses';
import { createStage, type Stage as Scene } from './stageScene';

/** Hover tint: the texel under the pointer, lifted toward white so it reads on any colour. */
function withHover(pixels: Pixels, hover: [number, number] | null): Uint8ClampedArray {
  if (!hover) return pixels.data;
  const data = new Uint8ClampedArray(pixels.data);
  const i = (hover[1] * SKIN_SIZE + hover[0]) * 4;
  const lift = (c: number) => c + (255 - c) * 0.55;
  for (let k = 0; k < 3; k++) data[i + k] = lift(data[i + k]!);
  data[i + 3] = 255;
  return data;
}

function tryStage(canvas: HTMLCanvasElement): Scene | null {
  try {
    return createStage(canvas);
  } catch {
    return null;
  }
}

export interface StageHandle {
  resetView(): void;
  zoom(factor: number): void;
  celebrate(): void;
}

/** The 3D figure. Drag on it to paint; drag the empty stage, or right-drag anywhere, to turn it; middle-drag or
 *  Shift-drag to slide it; the wheel zooms toward the pointer. */
export function Stage({
  pixels,
  rev,
  model,
  hidden,
  paintLayer,
  pose,
  paused,
  hover,
  onHover,
  onBegin,
  onPaint,
  onEnd,
  ref,
}: {
  pixels: Pixels | null;
  rev: number;
  model: SkinModel;
  hidden: Set<string>;
  paintLayer: Layer;
  pose: Pose;
  paused: boolean;
  hover: [number, number] | null;
  onHover: (texel: [number, number] | null) => void;
  onBegin: () => void;
  onPaint: (x: number, y: number) => void;
  onEnd: () => void;
  ref?: Ref<StageHandle>;
}) {
  const { t } = useTranslation();
  const canvas = useRef<HTMLCanvasElement>(null);
  const [scene, setScene] = useState<Scene | null>(null);
  const drag = useRef<{ mode: 'paint' | 'orbit' | 'pan'; x: number; y: number } | null>(null);

  useEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const made = tryStage(el);
    if (!made) return; // No WebGL: the flat view still edits everything.
    setScene(made);
    const fit = () => made.resize(el.clientWidth, el.clientHeight);
    fit();
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(fit);
    observer?.observe(el);
    return () => {
      observer?.disconnect();
      made.dispose();
      setScene(null);
    };
  }, []);
  useImperativeHandle(ref, () => ({ resetView: () => scene?.resetView(), zoom: (f) => scene?.zoom(f), celebrate: () => scene?.celebrate() }), [scene]);
  useEffect(() => scene?.setPose(pose), [scene, pose]);
  useEffect(() => scene?.setPaused(paused), [scene, paused]);

  useEffect(() => scene?.setModel(model), [scene, model]);
  useEffect(() => scene?.setPaintLayer(paintLayer), [scene, paintLayer]);
  useEffect(() => {
    const s = scene;
    if (!s) return;
    for (const part of ['head', 'body', 'rightArm', 'leftArm', 'rightLeg', 'leftLeg'] as Part[]) {
      for (const layer of ['base', 'outer'] as Layer[]) s.setVisible(part, layer, !hidden.has(`${part}:${layer}`));
    }
  }, [scene, hidden, model]);
  useEffect(() => {
    if (pixels) scene?.setTexture(withHover(pixels, hover));
  }, [scene, pixels, rev, hover, model]);

  const down = (event: PointerEvent<HTMLCanvasElement>) => {
    const panning = event.button === 1 || (event.button === 0 && event.shiftKey);
    const hit = event.button === 0 && !panning ? scene?.pick(event.clientX, event.clientY) : null;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    if (panning) {
      // The middle button would otherwise start the browser's autoscroll.
      event.preventDefault();
      drag.current = { mode: 'pan', x: event.clientX, y: event.clientY };
    } else if (hit) {
      drag.current = { mode: 'paint', x: event.clientX, y: event.clientY };
      onBegin();
      onPaint(hit.x, hit.y);
    } else {
      drag.current = { mode: 'orbit', x: event.clientX, y: event.clientY };
    }
  };
  const move = (event: PointerEvent<HTMLCanvasElement>) => {
    const s = scene;
    if (!s) return;
    const d = drag.current;
    if (d?.mode === 'orbit' || d?.mode === 'pan') {
      if (d.mode === 'orbit') s.orbit(event.clientX - d.x, event.clientY - d.y);
      else s.pan(event.clientX - d.x, event.clientY - d.y);
      d.x = event.clientX;
      d.y = event.clientY;
      return;
    }
    const hit = s.pick(event.clientX, event.clientY);
    onHover(hit ? [hit.x, hit.y] : null);
    if (d?.mode === 'paint' && hit) onPaint(hit.x, hit.y);
  };
  const up = () => {
    if (drag.current?.mode === 'paint') onEnd();
    drag.current = null;
  };
  const wheel = (event: WheelEvent) => scene?.zoom(event.deltaY > 0 ? 1.1 : 1 / 1.1, event.clientX, event.clientY);

  return (
    <canvas
      ref={canvas}
      className="stage-canvas"
      data-testid="stage"
      role="img"
      aria-label={t('skins.stageLabel')}
      onPointerDown={down}
      onMouseDown={(e) => e.button === 1 && e.preventDefault()}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      onPointerLeave={() => !drag.current && onHover(null)}
      onWheel={wheel}
      onContextMenu={(e) => e.preventDefault()}
    />
  );
}
