// Skin textures cross the IPC as PNG `data:` URLs; the editor works on raw texels. The webview's canvas does the
// conversion both ways. Tests mock this module (jsdom has no canvas).
import { SKIN_SIZE } from './layout';

function canvas2d(): [HTMLCanvasElement, CanvasRenderingContext2D] {
  const canvas = document.createElement('canvas');
  canvas.width = SKIN_SIZE;
  canvas.height = SKIN_SIZE;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) throw new Error('no 2D canvas');
  return [canvas, ctx];
}

/** The 64 × 64 RGBA texels of a PNG data URL (the backend has already made it 64 × 64). */
export function decodeTexture(url: string): Promise<Uint8ClampedArray> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      const [, ctx] = canvas2d();
      ctx.clearRect(0, 0, SKIN_SIZE, SKIN_SIZE);
      ctx.drawImage(image, 0, 0);
      resolve(ctx.getImageData(0, 0, SKIN_SIZE, SKIN_SIZE).data);
    };
    image.onerror = () => reject(new Error('the skin texture could not be read'));
    image.src = url;
  });
}

/** `data`, 64 × 64 RGBA texels, as a PNG data URL. */
export function encodeTexture(data: Uint8ClampedArray): string {
  const [canvas, ctx] = canvas2d();
  const image = ctx.createImageData(SKIN_SIZE, SKIN_SIZE);
  image.data.set(data);
  ctx.putImageData(image, 0, 0);
  return canvas.toDataURL('image/png');
}
