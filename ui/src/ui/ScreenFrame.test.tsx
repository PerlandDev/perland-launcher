import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import { Dialog } from './Dialog';
import { ScreenFrame } from './ScreenFrame';

it('goes back with the back key, which names its shortcut', async () => {
  const onBack = vi.fn();
  render(<ScreenFrame title="Settings" onBack={onBack}>{null}</ScreenFrame>);
  const back = screen.getByRole('button', { name: 'Back' });
  expect(back).toHaveAttribute('aria-keyshortcuts', 'Alt+ArrowLeft');
  await userEvent.click(back);
  expect(onBack).toHaveBeenCalledOnce();
});

it('goes back on Alt+Left', async () => {
  const onBack = vi.fn();
  render(<ScreenFrame title="Settings" onBack={onBack}>{null}</ScreenFrame>);
  await userEvent.keyboard('{Alt>}{ArrowLeft}{/Alt}');
  expect(onBack).toHaveBeenCalledOnce();
});

it('leaves Alt+Left alone in a text field, where it moves the caret', async () => {
  const onBack = vi.fn();
  render(
    <ScreenFrame title="Settings" onBack={onBack}>
      <input aria-label="Name" />
    </ScreenFrame>,
  );
  await userEvent.click(screen.getByRole('textbox', { name: 'Name' }));
  await userEvent.keyboard('{Alt>}{ArrowLeft}{/Alt}');
  expect(onBack).not.toHaveBeenCalled();
});

it('leaves Alt+Left alone while a dialog is open over the screen', async () => {
  const onBack = vi.fn();
  render(
    <ScreenFrame title="Settings" onBack={onBack}>
      <Dialog open title="Remove" onClose={() => {}} actions={<button type="button">OK</button>} />
    </ScreenFrame>,
  );
  await userEvent.keyboard('{Alt>}{ArrowLeft}{/Alt}');
  expect(onBack).not.toHaveBeenCalled();
});

it('has no back key or shortcut without somewhere to go', async () => {
  render(<ScreenFrame title="Home">{null}</ScreenFrame>);
  expect(screen.queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
});

// A visually hidden radio is `position: absolute`, so it belongs to its nearest positioned ancestor. If that sits
// outside the scroller, focusing the radio scrolls the shell's `main` instead and drags the whole screen off (#121).
it('keeps a hidden control positioned inside its scroller, so focusing it scrolls the screen and not the shell', () => {
  render(
    <ScreenFrame title="Settings">
      <label className="seg">
        <input type="radio" className="sr-only" aria-label="Custom size" />
        Custom size
      </label>
    </ScreenFrame>,
  );
  const radio = screen.getByRole('radio', { name: 'Custom size' });
  const scroller = radio.closest('.overflow-auto');
  const containingBlock = radio.parentElement?.closest('.relative, .absolute, .fixed, .sticky');
  expect(scroller).not.toBeNull();
  expect(containingBlock === scroller || scroller!.contains(containingBlock ?? null)).toBe(true);
});
