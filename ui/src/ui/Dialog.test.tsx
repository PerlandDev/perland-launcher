import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import { Dialog } from './Dialog';

it('puts a back key before the title when the dialog has a step to go back to', async () => {
  const onBack = vi.fn();
  render(
    <Dialog open onClose={() => {}} title="Adjust the background" back={{ label: 'Back', onBack }}>
      body
    </Dialog>,
  );
  const dialog = screen.getByRole('dialog', { name: 'Adjust the background', hidden: true });
  const back = screen.getByRole('button', { name: 'Back', hidden: true });
  // It leads the title row.
  expect(back.compareDocumentPosition(screen.getByRole('heading', { name: 'Adjust the background', hidden: true })) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(dialog).toContainElement(back);
  await userEvent.click(back);
  expect(onBack).toHaveBeenCalledOnce();
});

it('has no back key otherwise', () => {
  render(<Dialog open onClose={() => {}} title="Change image" />);
  expect(screen.queryByRole('button', { name: 'Back', hidden: true })).toBeNull();
});
