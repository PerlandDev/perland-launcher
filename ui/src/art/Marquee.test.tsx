import { act, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Marquee } from './Marquee';

const pixelEdgesMock = vi.hoisted(() => vi.fn<(src: string | null) => string | null>(() => null));
vi.mock('./pixelEdges', () => ({ usePixelEdges: pixelEdgesMock }));

const BANNER = 'data:image/png;base64,AAAA';

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

  it('keeps the plain cover path at 100', () => {
    const { img } = zoomed({ zoom: 100 });
    expect(img.style.width).toBe('');
    expect(img.hasAttribute('data-zoomed')).toBe(false);
  });

  it('sizes and places the picture itself below 100', () => {
    const { img } = zoomed({ zoom: 50 });
    expect([img.style.left, img.style.top, img.style.width, img.style.height]).toEqual(['250px', '0px', '500px', '500px']);
    expect(img.hasAttribute('data-zoomed')).toBe(true);
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
    expect(zoomed({ zoom: 150, edges: 'pixel' }).container.querySelector('img.marquee-edges')).toBeNull();
  });

  it('decides hard pixels from the size the zoom draws, not the panel', () => {
    // 600 px covering 1000 × 500 grows 1.67×: smooth. Zoomed to 150 it grows 2.5×: hard pixels.
    expect(zoomed({ zoom: null }, { width: 600, height: 600 }).img.hasAttribute('data-upscale')).toBe(false);
    expect(zoomed({ zoom: 150 }, { width: 600, height: 600 }).img.hasAttribute('data-upscale')).toBe(true);
  });
});
