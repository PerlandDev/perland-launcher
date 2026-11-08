import { afterEach, expect, it, vi } from 'vitest';
import { pixelEdges } from './pixelEdges';

afterEach(() => vi.restoreAllMocks());

it('gives no picture where there is no 2D canvas', async () => {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null);
  await expect(pixelEdges('data:image/png;base64,AAAA')).resolves.toBeNull();
});
