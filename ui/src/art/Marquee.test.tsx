import { act, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Marquee } from './Marquee';

const pixelEdgesMock = vi.hoisted(() => vi.fn<(src: string | null) => string | null>(() => null));
vi.mock('./pixelEdges', () => ({ usePixelEdges: pixelEdgesMock }));

const BANNER = 'data:image/png;base64,AAAA';

it("paints the generated scene in the theme's own colours, and follows a theme change", async () => {
  const { marqueeScene } = await import('./marqueeScene');
  const { PALETTES } = await import('./palette');
  const sky = (root: Element) => root.querySelector('svg > rect')?.getAttribute('fill');
  try {
    document.documentElement.dataset.theme = 'night';
    const { container } = render(<Marquee seed="asalith-fields" attract={false} />);
    expect(sky(container)).toBe(marqueeScene('asalith-fields', PALETTES.night).sky);
    await act(async () => {
      document.documentElement.dataset.theme = 'forest';
    });
    expect(sky(container)).toBe(marqueeScene('asalith-fields', PALETTES.forest).sky);
  } finally {
    delete document.documentElement.dataset.theme;
  }
});

it("draws a pack's banner in place of the generated scene", () => {
  const { container } = render(<Marquee seed="sky-islands" attract={false} banner={BANNER} />);
  expect(container.querySelector('img.marquee-banner')).not.toBeNull();
  expect(container.querySelector('.marquee-layer')).toBeNull();
});

it('falls back to the generated scene when the banner fails to decode', () => {
  const { container } = render(<Marquee seed="sky-islands" attract={false} banner={BANNER} />);
  fireEvent.error(container.querySelector('img.marquee-banner')!);
  expect(container.querySelector('img.marquee-banner')).toBeNull();
  expect(container.querySelectorAll('.marquee-layer').length).toBeGreaterThan(0);
});

it("moves a banner's position to the player's focus, and leaves the default to the stylesheet", () => {
  const { container, rerender } = render(<Marquee seed="sky-islands" attract={false} banner={BANNER} focus={{ x: 20, y: 80 }} />);
  const img = () => container.querySelector<HTMLImageElement>('img.marquee-banner')!;
  expect(img().style.objectPosition).toBe('20% 80%');
  // Back to the default: the inline position goes, so `.marquee-banner`'s center bottom applies.
  rerender(<Marquee seed="sky-islands" attract={false} banner={BANNER} focus={null} />);
  expect(img().style.objectPosition).toBe('');
});

it('has no inline position without a focus', () => {
  const { container } = render(<Marquee seed="sky-islands" attract={false} banner={BANNER} />);
  expect(container.querySelector('img.marquee-banner')!.getAttribute('style')).toBeNull();
});

describe('a zoomed banner', () => {
  const frameSize = { width: 1000, height: 500 };
  let edges: string | null = null;
  beforeEach(() => {
    edges = 'data:image/png;base64,EDGES';
    pixelEdgesMock.mockImplementation((src: string | null) => (src ? edges : null));
    // The panel measures 1000 × 500.
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor(private cb: ResizeObserverCallback) {}
        observe() {
          this.cb([{ contentRect: frameSize } as ResizeObserverEntry], this as unknown as ResizeObserver);
        }
        disconnect() {}
        unobserve() {}
      },
    );
  });
  afterEach(() => vi.unstubAllGlobals());

  /** Render, then let the banner "load" at the given natural size. */
  function zoomed(props: Partial<Parameters<typeof Marquee>[0]>, natural = { width: 500, height: 500 }) {
    const view = render(<Marquee seed="sky-islands" attract={false} banner={BANNER} {...props} />);
    const img = view.container.querySelector<HTMLImageElement>('img.marquee-banner')!;
    Object.defineProperty(img, 'naturalWidth', { value: natural.width });
    Object.defineProperty(img, 'naturalHeight', { value: natural.height });
    act(() => {
      fireEvent.load(img);
    });
    return { ...view, img };
  }

  it('keeps the plain cover path while no zoom is chosen', () => {
    const { img } = zoomed({ zoom: null });
    expect(img.style.width).toBe('');
    expect(img.hasAttribute('data-zoomed')).toBe(false);
  });

  it("sizes a chosen zoom from the panel's height and places the picture itself", () => {
    const { img } = zoomed({ zoom: 50 });
    expect([img.style.left, img.style.top, img.style.width, img.style.height]).toEqual(['375px', '250px', '250px', '250px']);
    expect(img.hasAttribute('data-zoomed')).toBe(true);
    // 200 is as tall as twice the panel: 1000 × 1000, which covers it.
    expect(zoomed({ zoom: 200 }).img.style.width).toBe('1000px');
  });

  it('draws pixel edges behind it, and none when they cannot be made', () => {
    const { container } = zoomed({ zoom: 50, edges: 'pixel' });
    expect(container.querySelector<HTMLImageElement>('img.marquee-edges')?.getAttribute('src')).toBe('data:image/png;base64,EDGES');
    expect(pixelEdgesMock).toHaveBeenLastCalledWith(BANNER);
    edges = null;
    const again = zoomed({ zoom: 50, edges: 'pixel' });
    expect(again.container.querySelector('img.marquee-edges')).toBeNull();
  });

  it('draws no edges for dark edges or a picture that still covers', () => {
    expect(zoomed({ zoom: 50, edges: 'dark' }).container.querySelector('img.marquee-edges')).toBeNull();
    expect(zoomed({ zoom: 200, edges: 'pixel' }).container.querySelector('img.marquee-edges')).toBeNull();
    // 150 is 750 wide in a 1000 wide panel: the sides show.
    expect(zoomed({ zoom: 150, edges: 'pixel' }).container.querySelector('img.marquee-edges')).not.toBeNull();
  });

  it('decides hard pixels from the size the zoom draws, not the panel', () => {
    // 600 px covering 1000 × 500 grows 1.67×: smooth. Zoomed to 300 (1500 px tall) it grows 2.5×: hard pixels.
    expect(zoomed({ zoom: null }, { width: 600, height: 600 }).img.hasAttribute('data-upscale')).toBe(false);
    expect(zoomed({ zoom: 300 }, { width: 600, height: 600 }).img.hasAttribute('data-upscale')).toBe(true);
  });
});
