import { expect, test } from 'vitest';
import en from '../../i18n/en.json';
import es from '../../i18n/es.json';
import { SLOTS } from './compose';
import { CATEGORIES } from './categories';
import { focusFor, FOCUSES } from './framing';

test('every slot of the catalog is dressed from exactly one Create category', () => {
  const dressed = CATEGORIES.flatMap((c) => c.slots);
  expect([...dressed].sort()).toEqual([...SLOTS].sort());
});

test.each([
  ['en', en],
  ['es', es],
])('every category and slot shown has a name in %s', (_lang, strings) => {
  const { create, catalog } = strings.skins;
  for (const category of CATEGORIES) {
    expect(create.category, category.id).toHaveProperty(category.id);
    // A category of several slots heads each group with the slot's own name.
    if (category.slots.length > 1) for (const slot of category.slots) expect(create.slot, slot).toHaveProperty(slot);
  }
  // A look's page lists what it wears by slot.
  for (const slot of SLOTS) expect(catalog.slot, slot).toHaveProperty(slot);
});

test('the camera has a shot for every category, and for every slot of Accessories', () => {
  for (const category of CATEGORIES) {
    expect(FOCUSES).toContain(focusFor(category.id));
    for (const slot of category.slots) expect(FOCUSES).toContain(focusFor(category.id, slot));
  }
  // Each accessory is framed where it is worn, not as the whole figure.
  const accessories = CATEGORIES.find((c) => c.id === 'accessories')!;
  expect(accessories.slots.map((slot) => focusFor('accessories', slot))).not.toContain('figure');
});
