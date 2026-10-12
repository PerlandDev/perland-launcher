import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import { Menu } from './Menu';

it('shows a disabled item but never chooses it, and the arrow keys skip it', async () => {
  const user = userEvent.setup();
  const [first, off, last] = [vi.fn(), vi.fn(), vi.fn()];
  render(
    <Menu
      label="More options"
      items={[
        { label: 'Off', onSelect: off, disabled: true },
        { label: 'First', onSelect: first },
        { label: 'Last', onSelect: last },
      ]}
    />,
  );
  await user.click(screen.getByRole('button', { name: 'More options' }));
  expect(screen.getByRole('menuitem', { name: 'Off' })).toBeDisabled();
  expect(screen.getByRole('menuitem', { name: 'First' })).toHaveFocus();
  await user.keyboard('{ArrowUp}');
  expect(screen.getByRole('menuitem', { name: 'Last' })).toHaveFocus();
  await user.keyboard('{ArrowDown}');
  expect(screen.getByRole('menuitem', { name: 'First' })).toHaveFocus();
  await user.keyboard('{Enter}');
  expect(first).toHaveBeenCalledOnce();
  await user.click(screen.getByRole('button', { name: 'More options' }));
  await user.click(screen.getByRole('menuitem', { name: 'Off' }));
  expect(off).not.toHaveBeenCalled();
});
