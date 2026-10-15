import { act, screen, within } from '@testing-library/react';
import i18n from '../../i18n';
import { describe, expect, it, vi } from 'vitest';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { Home } from './Home';

async function openPicker(user: ReturnType<typeof renderApp>['user'], name: string) {
  await screen.findByRole('heading', { name });
  await user.click(screen.getByRole('button', { name: 'More options' }));
  await user.click(screen.getByRole('menuitem', { name: 'Change image…' }));
  return screen.getByRole('dialog', { name: 'Change image' });
}

describe('Change image', () => {
  it('sends the third landscape and lights it', async () => {
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    const tiles = within(dialog).getAllByRole('button', { name: /^Landscape \d$/ });
    expect(tiles).toHaveLength(6);
    // The instance has no pinned seed, so its own id (the first landscape) is the current one.
    expect(within(dialog).getByRole('button', { name: 'Landscape 1' })).toHaveAttribute('aria-pressed', 'true');
    const artReads = sent(calls, 'instance_art').length;
    await user.click(within(dialog).getByRole('button', { name: 'Landscape 3' }));
    expect(sent(calls, 'instance_art_set')).toEqual([{ id: 'asalith-fields', choice: { kind: 'generated', seed: 'asalith-fields~3' } }]);
    expect(await within(dialog).findByRole('button', { name: 'Landscape 3' })).toHaveAttribute('aria-pressed', 'true');
    expect(within(dialog).getByRole('button', { name: 'Landscape 1' })).toHaveAttribute('aria-pressed', 'false');
    // The art is read again, so the rail and the marquee follow.
    await vi.waitFor(() => expect(sent(calls, 'instance_art').length).toBeGreaterThan(artReads));
  });

  it('offers the pack art only for a pack, lit while it is in use', async () => {
    const first = renderApp(<Home id="asalith-fields" />);
    const plain = await openPicker(first.user, 'Asalith Fields');
    expect(within(plain).queryByRole('button', { name: 'Pack art' })).not.toBeInTheDocument();
    expect(within(plain).getByRole('button', { name: 'Upload your own…' })).toHaveAttribute('aria-pressed', 'false');
    first.unmount();

    const { user, calls } = renderApp(<Home id="sky-islands" />);
    const pack = await openPicker(user, 'Sky Islands');
    const packArt = within(pack).getByRole('button', { name: 'Pack art' });
    expect(packArt).toHaveAttribute('aria-pressed', 'true');
    for (const tile of within(pack).getAllByRole('button', { name: /^Landscape \d$/ })) expect(tile).toHaveAttribute('aria-pressed', 'false');
    await user.click(within(pack).getByRole('button', { name: 'Landscape 2' }));
    await user.click(await within(pack).findByRole('button', { name: 'Pack art' }));
    expect(sent(calls, 'instance_art_set').at(-1)).toEqual({ id: 'sky-islands', choice: { kind: 'pack' } });
  });

  it('uploads a picked image and lights Upload', async () => {
    // The mock's file picker answers an image filter with a picture.
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(within(dialog).getByRole('button', { name: 'Upload your own…' }));
    const [options] = sent(calls, 'plugin:dialog|open') as { options: { filters: { extensions: string[] }[] } }[];
    expect(options?.options.filters[0]?.extensions).toEqual(['png', 'jpg', 'jpeg', 'webp']);
    expect(sent(calls, 'instance_art_set')).toEqual([{ id: 'asalith-fields', choice: { kind: 'upload', path: 'C:/Users/player/Pictures/castle.png' } }]);
    expect(await within(dialog).findByRole('button', { name: 'Upload your own…' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('shows a refused image as a translated error', async () => {
    const { user, calls, backend } = renderApp(<Home id="asalith-fields" />);
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
      cmd === 'plugin:dialog|open' ? Promise.resolve('C:/Users/player/Pictures/invalid.png') : handle(cmd, args),
    );
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(within(dialog).getByRole('button', { name: 'Upload your own…' }));
    const alert = await within(dialog).findByRole('alert');
    expect(alert).toHaveTextContent("That file can't be used as an instance image.");
    expect(alert).toHaveTextContent('Pick a PNG, JPEG or WebP image under 5 MB.');
    expect(sent(calls, 'instance_art_set')).toHaveLength(1);
    // Nothing changed, so Landscape 1 is still the current one.
    expect(within(dialog).getByRole('button', { name: 'Landscape 1' })).toHaveAttribute('aria-pressed', 'true');
  });

  it('does nothing when the file picker is closed', async () => {
    const { user, calls, backend } = renderApp(<Home id="asalith-fields" />);
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'plugin:dialog|open' ? Promise.resolve(null) : handle(cmd, args)));
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(within(dialog).getByRole('button', { name: 'Upload your own…' }));
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
  ['en', 'Upload your own…', 'PNG, JPEG or WebP, up to 5 MB'],
  ['es', 'Sube la tuya…', 'PNG, JPEG o WebP, hasta 5 MB'],
])('states what an upload may be before picking (%s)', async (lang, upload, limits) => {
  await act(() => i18n.changeLanguage(lang));
  const { user } = renderApp(<Home id="asalith-fields" />);
  await screen.findByRole('heading', { name: 'Asalith Fields' });
  await user.click(screen.getByRole('button', { name: lang === 'en' ? 'More options' : 'Más opciones' }));
  await user.click(screen.getByRole('menuitem', { name: lang === 'en' ? 'Change image…' : 'Cambiar imagen…' }));
  const key = screen.getByRole('button', { name: upload });
  expect(key).toHaveTextContent(limits);
  expect(key).toHaveAccessibleDescription(limits);
});
