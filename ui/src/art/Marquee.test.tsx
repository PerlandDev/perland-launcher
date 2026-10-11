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
