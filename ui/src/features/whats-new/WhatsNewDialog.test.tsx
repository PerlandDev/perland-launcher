import { act, screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import i18n from '../../i18n';
import type { NoteGroup } from '../../ipc/schema';
import { renderWithProviders } from '../../test/render';
import { WhatsNewDialog } from './WhatsNewDialog';

const newest: NoteGroup = {
  base: '2026.11.6',
  date: '2026-11-20',
  notes: [
    { id: 'c', kind: 'new', since: '2026.11.6', en: 'You can draw your own skin.', es: 'Puedes dibujar tu propia skin.' },
    { id: 'd', kind: 'fixed', since: '2026.11.6', en: 'Text <b>x</b> stays text.', es: 'El texto <b>x</b> sigue siendo texto.' },
  ],
};
const older: NoteGroup = {
  base: '2026.11.5',
  date: '2026-11-12',
  notes: [{ id: 'a', kind: 'improved', since: '2026.11.5', en: 'Skins save faster.', es: 'Las skins se guardan más rápido.' }],
};
const next: NoteGroup = { base: null, date: '2026-11-21', notes: [{ id: 'e', kind: 'new', since: null, en: 'Coming soon.', es: 'Pronto.' }] };

it('lists each version with its day, and its notes by kind', () => {
  renderWithProviders(<WhatsNewDialog open groups={[newest, older]} all={null} onClose={() => {}} />);
  const dialog = screen.getByRole('dialog', { name: "What's new" });
  expect(within(dialog).getAllByRole('heading', { level: 3 }).map((h) => h.textContent)).toEqual(['2026.11.6 · Nov 20', '2026.11.5 · Nov 12']);
  // Only the kinds a version has, New before Improved before Fixed.
  expect(within(dialog).getAllByRole('heading', { level: 4 }).map((h) => h.textContent)).toEqual(['New', 'Fixed', 'Improved']);
  expect(within(dialog).getAllByRole('listitem').map((li) => li.textContent)).toEqual(['You can draw your own skin.', 'Text <b>x</b> stays text.', 'Skins save faster.']);
});

it('shows a note as plain text, never as HTML', () => {
  renderWithProviders(<WhatsNewDialog open groups={[newest]} all={null} onClose={() => {}} />);
  expect(screen.getByText('Text <b>x</b> stays text.')).toBeInTheDocument();
});

it("follows the launcher's language", async () => {
  renderWithProviders(<WhatsNewDialog open groups={[newest, next]} all={null} onClose={() => {}} />);
  await act(() => i18n.changeLanguage('es'));
  const dialog = screen.getByRole('dialog', { name: 'Novedades del launcher' });
  expect(within(dialog).getAllByRole('heading', { level: 3 }).map((h) => h.textContent)).toEqual(['2026.11.6 · 20 nov', 'Próxima actualización']);
  expect(within(dialog).getAllByRole('heading', { level: 4 }).map((h) => h.textContent)).toEqual(['Nuevo', 'Arreglos', 'Nuevo']);
  expect(within(dialog).getByText('Puedes dibujar tu propia skin.')).toBeInTheDocument();
  expect(within(dialog).queryByText('You can draw your own skin.')).not.toBeInTheDocument();
  await act(() => i18n.changeLanguage('en'));
});

it('offers All updates only when there is more to show, and shows it', async () => {
  const { user, rerender } = renderWithProviders(<WhatsNewDialog open groups={[newest]} all={[newest]} onClose={() => {}} />);
  expect(screen.queryByRole('button', { name: 'All updates' })).not.toBeInTheDocument();
  rerender(<WhatsNewDialog open groups={[newest]} all={[newest, older]} onClose={() => {}} />);
  await user.click(screen.getByRole('button', { name: 'All updates' }));
  expect(screen.getByText('Skins save faster.')).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'All updates' })).not.toBeInTheDocument();
});

it('closes with Got it', async () => {
  const onClose = vi.fn();
  const { user } = renderWithProviders(<WhatsNewDialog open groups={[newest]} all={null} onClose={onClose} />);
  await user.click(screen.getByRole('button', { name: 'Got it' }));
  expect(onClose).toHaveBeenCalledOnce();
});

it('says so when there are no notes', () => {
  renderWithProviders(<WhatsNewDialog open groups={[]} all={null} onClose={() => {}} />);
  expect(screen.getByText('No notes yet.')).toBeInTheDocument();
});
