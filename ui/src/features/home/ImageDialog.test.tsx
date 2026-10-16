import { act, screen, within } from '@testing-library/react';
import i18n from '../../i18n';
import { describe, expect, it, vi } from 'vitest';
import { runStore } from '../../state/runStore';
import { createMockBackend } from '../../ipc/mock';
import type { Instance } from '../../ipc/schema';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { Home } from './Home';

async function openPicker(user: ReturnType<typeof renderApp>['user'], name: string) {
  await screen.findByRole('heading', { name });
  await user.click(screen.getByRole('button', { name: 'More options' }));
  await user.click(screen.getByRole('menuitem', { name: 'Change image…' }));
  return screen.getByRole('dialog', { name: 'Change image' });
}


const group = (dialog: HTMLElement, name: 'Icon' | 'Background') => within(within(dialog).getByRole('radiogroup', { name }));

describe('Change image', () => {
  it('sends the third landscape and lights it', async () => {
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    const tiles = group(dialog, 'Background').getAllByRole('radio', { name: /^Landscape \d$/ });
    expect(tiles).toHaveLength(6);
    // The instance has no pinned seed, so its own id (the first landscape) is the current one.
    expect(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 1' })).toHaveAttribute('aria-checked', 'true');
    const artReads = sent(calls, 'instance_art').length;
    await user.click(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 3' }));
    expect(sent(calls, 'instance_art_set')).toEqual([{ id: 'asalith-fields', choice: { kind: 'generated', seed: 'asalith-fields~3' } }]);
    expect(await group(dialog, 'Background').findByRole('radio', { name: 'Landscape 3' })).toHaveAttribute('aria-checked', 'true');
    expect(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 1' })).toHaveAttribute('aria-checked', 'false');
    // The art is read again, so the rail and the marquee follow.
    await vi.waitFor(() => expect(sent(calls, 'instance_art').length).toBeGreaterThan(artReads));
  });

  it('offers the pack art only for a pack, lit while it is in use', async () => {
    const first = renderApp(<Home id="asalith-fields" />);
    const plain = await openPicker(first.user, 'Asalith Fields');
    expect(group(plain, 'Background').queryByRole('radio', { name: 'Pack art' })).not.toBeInTheDocument();
    expect(group(plain, 'Background').getByRole('radio', { name: 'Upload your own…' })).toHaveAttribute('aria-checked', 'false');
    first.unmount();

    const { user, calls } = renderApp(<Home id="sky-islands" />);
    const pack = await openPicker(user, 'Sky Islands');
    const packArt = group(pack, 'Background').getByRole('radio', { name: 'Pack art' });
    expect(packArt).toHaveAttribute('aria-checked', 'true');
    for (const tile of group(pack, 'Background').getAllByRole('radio', { name: /^Landscape \d$/ })) expect(tile).toHaveAttribute('aria-checked', 'false');
    await user.click(group(pack, 'Background').getByRole('radio', { name: 'Landscape 2' }));
    await user.click(await group(pack, 'Background').findByRole('radio', { name: 'Pack art' }));
    expect(sent(calls, 'instance_art_set').at(-1)).toEqual({ id: 'sky-islands', choice: { kind: 'pack' } });
  });

  it('uploads a picked image and lights Upload', async () => {
    // The mock's file picker answers an image filter with a picture.
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(group(dialog, 'Background').getByRole('radio', { name: 'Upload your own…' }));
    const [options] = sent(calls, 'plugin:dialog|open') as { options: { filters: { extensions: string[] }[] } }[];
    expect(options?.options.filters[0]?.extensions).toEqual(['png', 'jpg', 'jpeg', 'webp']);
    expect(sent(calls, 'instance_art_set')).toEqual([{ id: 'asalith-fields', choice: { kind: 'upload', path: 'C:/Users/player/Pictures/castle.png' } }]);
    expect(await group(dialog, 'Background').findByRole('radio', { name: 'Upload your own…' })).toHaveAttribute('aria-checked', 'true');
  });

  it('shows a refused image as a translated error', async () => {
    const { user, calls, backend } = renderApp(<Home id="asalith-fields" />);
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
      cmd === 'plugin:dialog|open' ? Promise.resolve('C:/Users/player/Pictures/invalid.png') : handle(cmd, args),
    );
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(group(dialog, 'Background').getByRole('radio', { name: 'Upload your own…' }));
    const alert = await within(dialog).findByRole('alert');
    expect(alert).toHaveTextContent("That file can't be used as an instance image.");
    expect(alert).toHaveTextContent('Pick a PNG, JPEG or WebP image under 5 MB.');
    expect(sent(calls, 'instance_art_set')).toHaveLength(1);
    // Nothing changed, so Landscape 1 is still the current one.
    expect(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 1' })).toHaveAttribute('aria-checked', 'true');
  });

  it('does nothing when the file picker is closed', async () => {
    const { user, calls, backend } = renderApp(<Home id="asalith-fields" />);
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'plugin:dialog|open' ? Promise.resolve(null) : handle(cmd, args)));
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(group(dialog, 'Background').getByRole('radio', { name: 'Upload your own…' }));
    expect(sent(calls, 'instance_art_set')).toEqual([]);
    expect(within(dialog).queryByRole('alert')).not.toBeInTheDocument();
  });

  it("can't be opened while the game runs", async () => {
    const { user } = renderApp(<Home id="asalith-fields" />);
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
    await user.click(screen.getByRole('button', { name: 'More options' }));
    expect(screen.getByRole('menuitem', { name: 'Change image…' })).toBeDisabled();
  });

  it('closes with Done', async () => {
    const { user } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    expect(screen.queryByRole('dialog', { name: 'Change image' })).not.toBeInTheDocument();
  });
});

it.each([
  ['en', 'Upload your own…', 'Upload your own…', 'PNG, JPEG or WebP, up to 5 MB'],
  // "Tuyo" for the icon (el ícono), "tuya" for the background (la imagen).
  ['es', 'Sube el tuyo…', 'Sube la tuya…', 'PNG, JPEG o WebP, hasta 5 MB'],
])('states what an upload may be before picking (%s)', async (lang, iconUpload, upload, limits) => {
  await act(() => i18n.changeLanguage(lang));
  const { user } = renderApp(<Home id="asalith-fields" />);
  await screen.findByRole('heading', { name: 'Asalith Fields' });
  await user.click(screen.getByRole('button', { name: lang === 'en' ? 'More options' : 'Más opciones' }));
  await user.click(screen.getByRole('menuitem', { name: lang === 'en' ? 'Change image…' : 'Cambiar imagen…' }));
  const dialog = screen.getByRole('dialog');
  // Both sections take an upload, and both say what it may be.
  const keys = [...new Set([...screen.getAllByRole('radio', { name: iconUpload }), ...screen.getAllByRole('radio', { name: upload })])];
  expect(keys).toHaveLength(2);
  expect(within(dialog).getByRole('radiogroup', { name: lang === 'en' ? 'Icon' : 'Ícono' })).toContainElement(keys[0]!);
  expect(within(dialog).getByRole('radiogroup', { name: lang === 'en' ? 'Background' : 'Fondo' })).toContainElement(keys[1]!);
  for (const key of keys) {
    expect(key).toHaveTextContent(limits);
    // Tied with no-break spaces, so it can only break once: before "up to" / "hasta", never mid-list.
    const hint = key.querySelector('.art-key-hint')!.textContent!;
    const tied = (words: string) => words.replaceAll(' ', '\u00a0');
    expect(hint.split(' ')).toEqual(lang === 'en' ? [tied('PNG, JPEG or WebP,'), tied('up to 5 MB')] : [tied('PNG, JPEG o WebP,'), tied('hasta 5 MB')]);
    expect(key).toHaveAccessibleDescription(hint);
  }
});

describe('Change image: the icon', () => {
  it('splits the dialog into a labelled Icon group and a labelled Background group', async () => {
    const { user } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    const groups = within(dialog).getAllByRole('radiogroup');
    expect(groups).toHaveLength(2);
    expect(within(dialog).getByRole('radiogroup', { name: 'Icon' })).toBe(groups[0]);
    expect(within(dialog).getByRole('radiogroup', { name: 'Background' })).toBe(groups[1]);
    const icons = group(dialog, 'Icon').getAllByRole('radio');
    expect(icons).toHaveLength(2);
    expect(icons[0]).toHaveAccessibleName('Initials');
    expect(icons[1]).toHaveAccessibleName('Upload your own…');
    // The landscapes are backgrounds only.
    expect(group(dialog, 'Icon').queryByRole('radio', { name: /^Landscape/ })).not.toBeInTheDocument();
  });

  it('sends the initials as the generated icon, leaving the background alone', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    const dialog = await openPicker(user, 'Sky Islands');
    const icons = group(dialog, 'Icon');
    expect(icons.getByRole('radio', { name: 'Pack icon' })).toHaveAttribute('aria-checked', 'true');
    await user.click(icons.getByRole('radio', { name: 'Initials' }));
    expect(sent(calls, 'instance_icon_set')).toEqual([{ id: 'sky-islands', choice: { kind: 'generated' } }]);
    expect(sent(calls, 'instance_art_set')).toEqual([]);
    expect(await group(dialog, 'Icon').findByRole('radio', { name: 'Initials' })).toHaveAttribute('aria-checked', 'true');
    expect(group(dialog, 'Icon').getByRole('radio', { name: 'Pack icon' })).toHaveAttribute('aria-checked', 'false');
    // The background is still the pack's art.
    expect(group(dialog, 'Background').getByRole('radio', { name: 'Pack art' })).toHaveAttribute('aria-checked', 'true');
    await user.click(group(dialog, 'Icon').getByRole('radio', { name: 'Pack icon' }));
    expect(sent(calls, 'instance_icon_set').at(-1)).toEqual({ id: 'sky-islands', choice: { kind: 'pack' } });
  });

  it('offers the pack icon only for a pack', async () => {
    const first = renderApp(<Home id="asalith-fields" />);
    const plain = await openPicker(first.user, 'Asalith Fields');
    expect(group(plain, 'Icon').queryByRole('radio', { name: 'Pack icon' })).not.toBeInTheDocument();
    first.unmount();
    const { user } = renderApp(<Home id="sky-islands" />);
    const pack = await openPicker(user, 'Sky Islands');
    expect(group(pack, 'Icon').getByRole('radio', { name: 'Pack icon' })).toBeInTheDocument();
  });

  it('uploads a picked image as the icon only', async () => {
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(group(dialog, 'Icon').getByRole('radio', { name: 'Upload your own…' }));
    const [options] = sent(calls, 'plugin:dialog|open') as { options: { filters: { extensions: string[] }[] } }[];
    expect(options?.options.filters[0]?.extensions).toEqual(['png', 'jpg', 'jpeg', 'webp']);
    expect(sent(calls, 'instance_icon_set')).toEqual([{ id: 'asalith-fields', choice: { kind: 'upload', path: 'C:/Users/player/Pictures/castle.png' } }]);
    expect(sent(calls, 'instance_art_set')).toEqual([]);
    const key = await group(dialog, 'Icon').findByRole('radio', { name: 'Upload your own…' });
    expect(key).toHaveAttribute('aria-checked', 'true');
    // The key shows the icon now in use.
    await vi.waitFor(() => expect(key.querySelector('img')?.getAttribute('src')).toMatch(/^data:image\/png;base64,/));
    expect(group(dialog, 'Background').getByRole('radio', { name: 'Upload your own…' })).toHaveAttribute('aria-checked', 'false');
  });

  it.each<[string, Partial<Instance>, string, string]>([
    ['a never-chosen icon on a plain instance', { icon: null, art: 'auto' }, 'asalith-fields', 'Initials'],
    ['a never-chosen icon on a pack', { icon: null, art: 'auto' }, 'sky-islands', 'Pack icon'],
    ['a round-1 upload, which is still the icon', { icon: null, art: 'custom' }, 'asalith-fields', 'Upload your own…'],
    ['the initials on a pack', { icon: 'generated' }, 'sky-islands', 'Initials'],
    ['auto on a plain instance', { icon: 'auto', art: 'generated' }, 'asalith-fields', 'Initials'],
    ['an uploaded icon over generated art', { icon: 'custom', art: 'generated' }, 'asalith-fields', 'Upload your own…'],
  ])('lights the current icon for %s', async (_, patch, id, lit) => {
    const backend = createMockBackend();
    backend.state.listing.instances = backend.state.listing.instances.map((i) => (i.id === id ? { ...i, ...patch } : i));
    const name = backend.state.listing.instances.find((i) => i.id === id)!.name;
    const { user } = renderWithProviders(<Home id={id} />, { ipc: (cmd, args) => backend.handle(cmd, args) });
    const dialog = await openPicker(user, name);
    const checked = group(dialog, 'Icon')
      .getAllByRole('radio')
      .filter((r) => r.getAttribute('aria-checked') === 'true');
    expect(checked).toHaveLength(1);
    expect(checked[0]).toHaveAccessibleName(lit);
  });

  it('shows a refused icon as a translated error', async () => {
    const { user, calls, backend } = renderApp(<Home id="asalith-fields" />);
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
      cmd === 'plugin:dialog|open' ? Promise.resolve('C:/Users/player/Pictures/invalid.png') : handle(cmd, args),
    );
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(group(dialog, 'Icon').getByRole('radio', { name: 'Upload your own…' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent("That file can't be used as an instance image.");
    expect(sent(calls, 'instance_icon_set')).toHaveLength(1);
    expect(group(dialog, 'Icon').getByRole('radio', { name: 'Initials' })).toHaveAttribute('aria-checked', 'true');
  });

  it('holds both groups while either save is on its way', async () => {
    const { user, calls, backend } = renderApp(<Home id="sky-islands" />);
    const handle = backend.handle;
    let release = () => {};
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => {
      if (cmd !== 'instance_icon_set') return handle(cmd, args);
      return new Promise((resolve) => (release = () => resolve(handle(cmd, args))));
    });
    const dialog = await openPicker(user, 'Sky Islands');
    await user.click(group(dialog, 'Icon').getByRole('radio', { name: 'Initials' }));
    for (const radio of within(dialog).getAllByRole('radio')) expect(radio).toHaveAttribute('aria-disabled', 'true');
    // A background picked meanwhile would race the icon's save to instance.json, so it waits.
    await user.click(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 2' }));
    await user.click(group(dialog, 'Icon').getByRole('radio', { name: 'Pack icon' }));
    expect(sent(calls, 'instance_art_set')).toEqual([]);
    expect(sent(calls, 'instance_icon_set')).toHaveLength(1);
    await act(async () => release());
    await vi.waitFor(() => expect(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 2' })).not.toHaveAttribute('aria-disabled'));
  });

  it('holds both groups while a background saves, too', async () => {
    const { user, calls, backend } = renderApp(<Home id="sky-islands" />);
    const handle = backend.handle;
    let release = () => {};
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => {
      if (cmd !== 'instance_art_set') return handle(cmd, args);
      return new Promise((resolve) => (release = () => resolve(handle(cmd, args))));
    });
    const dialog = await openPicker(user, 'Sky Islands');
    await user.click(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 2' }));
    await user.click(group(dialog, 'Icon').getByRole('radio', { name: 'Initials' }));
    await user.click(group(dialog, 'Icon').getByRole('radio', { name: 'Upload your own…' }));
    expect(sent(calls, 'instance_icon_set')).toEqual([]);
    expect(sent(calls, 'plugin:dialog|open')).toEqual([]);
    await act(async () => release());
    expect(sent(calls, 'instance_art_set')).toHaveLength(1);
  });

  it('moves between choices with the arrow keys and picks with Space', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    const dialog = await openPicker(user, 'Sky Islands');
    const icons = group(dialog, 'Icon');
    // One stop per group: the current choice.
    expect(icons.getAllByRole('radio').filter((r) => r.tabIndex === 0)).toEqual([icons.getByRole('radio', { name: 'Pack icon' })]);
    icons.getByRole('radio', { name: 'Pack icon' }).focus();
    await user.keyboard('{ArrowLeft}');
    expect(icons.getByRole('radio', { name: 'Initials' })).toHaveFocus();
    // Moving never saves: an upload key would open the file picker.
    expect(sent(calls, 'instance_icon_set')).toEqual([]);
    await user.keyboard(' ');
    expect(sent(calls, 'instance_icon_set')).toEqual([{ id: 'sky-islands', choice: { kind: 'generated' } }]);
  });
});
