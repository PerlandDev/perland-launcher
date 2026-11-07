import { useEffect, useRef, type PointerEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type { SkinModel } from '../../ipc/schema';
import { boxes, SKIN_SIZE, type Layer } from './layout';
import type { Pixels } from './paint';

/** The flat 64 × 64 skin, upscaled with crisp texels, its faces outlined; paint on it like on the figure. */
export function TexturePanel({
  pixels,
  rev,
  model,
  hover,
  paintLayer,
  onHover,
  onBegin,
  onPaint,
  onEnd,
}: {
  pixels: Pixels | null;
  rev: number;
  model: SkinModel;
  hover: [number, number] | null;
  paintLayer: Layer;
  onHover: (texel: [number, number] | null) => void;
  onBegin: () => void;
  onPaint: (x: number, y: number, joined: boolean) => void;
  onEnd: () => void;
}) {
  const { t } = useTranslation();
  const canvas = useRef<HTMLCanvasElement>(null);
  const painting = useRef(false);

  useEffect(() => {
    const ctx = canvas.current?.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, SKIN_SIZE, SKIN_SIZE);
    if (!pixels) return;
    const image = ctx.createImageData(SKIN_SIZE, SKIN_SIZE);
    image.data.set(pixels.data);
    ctx.putImageData(image, 0, 0);
  }, [pixels, rev]);

  const texelAt = (event: PointerEvent): [number, number] | null => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width === 0) return null;
    const x = Math.floor(((event.clientX - rect.left) / rect.width) * SKIN_SIZE);
    const y = Math.floor(((event.clientY - rect.top) / rect.height) * SKIN_SIZE);
    return x >= 0 && y >= 0 && x < SKIN_SIZE && y < SKIN_SIZE ? [x, y] : null;
  };

  const down = (event: PointerEvent) => {
    if (event.button !== 0) return;
    const at = texelAt(event);
    if (!at) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    painting.current = true;
    onBegin();
    onPaint(at[0], at[1], false);
  };
  const move = (event: PointerEvent) => {
    const at = texelAt(event);
    onHover(at);
    if (painting.current && at) onPaint(at[0], at[1], true);
  };
  const up = () => {
    if (!painting.current) return;
    painting.current = false;
    onEnd();
  };

  const faces = boxes(model).flatMap((box) => box.faces.map((face) => ({ box, face })));
  return (
    <div className="texture-sheet" data-testid="texture-panel">
      <canvas ref={canvas} width={SKIN_SIZE} height={SKIN_SIZE} className="texture-canvas" aria-hidden />
      <svg
        viewBox={`0 0 ${SKIN_SIZE} ${SKIN_SIZE}`}
        className="texture-overlay"
        role="img"
        aria-label={t('skins.textureLabel')}
        onPointerDown={down}
        onPointerMove={move}
        onPointerUp={up}
        onPointerCancel={up}
        onPointerLeave={() => onHover(null)}
      >
        {faces.map(({ box, face }) => (
          <rect
            key={`${box.part}-${box.layer}-${face.name}`}
            x={face.x + 0.05}
            y={face.y + 0.05}
            width={face.w - 0.1}
            height={face.h - 0.1}
            className={box.layer === paintLayer ? 'texture-face' : 'texture-face texture-face-off'}
          />
        ))}
        {hover && <rect x={hover[0]} y={hover[1]} width={1} height={1} className="texture-hover" />}
      </svg>
    </div>
  );
}
