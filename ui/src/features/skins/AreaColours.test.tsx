import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import type { CatalogPiece } from '../../ipc/schema';
import { AreaColours } from './AreaColours';
import { CLOTH_COLOURS, HAIR_COLOURS, IRIS_COLOURS, SKIN_TONES } from './palettes';

const piece = (id: string, slot: string, areas: [string, string, string][]): CatalogPiece => ({
  id,
  slot,
  name: { en: id, es: id },
  tags: [],
  areas: areas.map(([a, name, d]) => ({ id: a, name: { en: name, es: name }, default: d })),
  files: {},
  thumb: '',
  retired: false,
});
const swatches = (area: string) => within(screen.getByRole('toolbar', { name: area })).getAllByRole('button');
const values = (area: string) => swatches(area).map((b) => b.getAttribute('title'));

function show(p: CatalogPiece, colours: Record<string, string> = {}) {
  const onColour = vi.fn();
  render(<AreaColours piece={p} worn={{ id: p.id, colours }} lang="en" onColour={onColour} />);
  return { onColour, user: userEvent.setup() };
}

it('each area gets the quick colours that suit it', () => {
  show(piece('body-plain', 'body', [['skin', 'Skin', '#d9a27c']]));
  expect(values('Skin')).toEqual([...SKIN_TONES]);
});

it('hair gets hair colours for its main and tips, eyes eye colours, clothes the cloth palette', () => {
  const { unmount } = render(
    <AreaColours piece={piece('hair-band', 'hair', [['main', 'Main', '#4b2f1f'], ['tips', 'Tips', '#7a5236'], ['band', 'Band', '#cc0000']])} worn={{ id: 'hair-band', colours: {} }} lang="en" onColour={() => {}} />,
  );
  expect(values('Main')).toEqual([...HAIR_COLOURS]);
  expect(values('Tips')).toEqual([...HAIR_COLOURS]);
  expect(values('Band')).toEqual([...CLOTH_COLOURS]);
  unmount();
  show(piece('eyes-round', 'eyes', [['iris', 'Iris', '#4a3324']]));
  expect(values('Iris')).toEqual([...IRIS_COLOURS]);
});

it('a swatch sets the colour in one step, and the one in use is marked', async () => {
  const { onColour, user } = show(piece('top-tee', 'top', [['main', 'Main', '#e8e4dc'], ['trim', 'Trim', '#3a6ea5']]), { main: CLOTH_COLOURS[4]!.toUpperCase(), trim: '#3a6ea5' });
  expect(values('Main')).toEqual([...CLOTH_COLOURS]);
  const pressed = swatches('Main').filter((b) => b.getAttribute('aria-pressed') === 'true');
  expect(pressed.map((b) => b.getAttribute('title'))).toEqual([CLOTH_COLOURS[4]]);
  // A colour of the picker's, not the palette's: no swatch is marked.
  expect(swatches('Trim').some((b) => b.getAttribute('aria-pressed') === 'true')).toBe(false);
  await user.click(screen.getByRole('button', { name: `Trim: ${CLOTH_COLOURS[2]}` }));
  expect(onColour).toHaveBeenCalledTimes(1);
  // Not `live`: its own undo step.
  expect(onColour).toHaveBeenCalledWith('trim', CLOTH_COLOURS[2], false);
});

it('the swatches of an area are one stop for Tab, walked with the arrow keys', async () => {
  const { onColour, user } = show(piece('top-tee', 'top', [['main', 'Main', '#e8e4dc']]), { main: CLOTH_COLOURS[3]! });
  const all = swatches('Main');
  expect(all.filter((b) => b.tabIndex === 0)).toEqual([all[3]]);
  all[3]!.focus();
  await user.keyboard('{ArrowRight}');
  expect(all[4]).toHaveFocus();
  await user.keyboard('{ArrowLeft}{ArrowLeft}');
  expect(all[2]).toHaveFocus();
  await user.keyboard('{End}');
  expect(all[all.length - 1]).toHaveFocus();
  await user.keyboard('{ArrowRight}');
  expect(all[0]).toHaveFocus();
  await user.keyboard('{Home}{ArrowLeft}');
  expect(all[all.length - 1]).toHaveFocus();
  // Moving is not choosing.
  expect(onColour).not.toHaveBeenCalled();
  await user.keyboard('{Enter}');
  expect(onColour).toHaveBeenCalledWith('main', CLOTH_COLOURS[CLOTH_COLOURS.length - 1], false);
});

it('with no swatch in use, the first one takes Tab', () => {
  show(piece('top-tee', 'top', [['main', 'Main', '#e8e4dc']]), { main: '#123456' });
  const all = swatches('Main');
  expect(all.filter((b) => b.tabIndex === 0)).toEqual([all[0]]);
});
