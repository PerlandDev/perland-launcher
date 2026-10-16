import { fireEvent, render } from '@testing-library/react';
import { expect, it } from 'vitest';
import { Marquee } from './Marquee';

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
