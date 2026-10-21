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
