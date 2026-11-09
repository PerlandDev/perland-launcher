import { act, fireEvent, screen, within } from '@testing-library/react';
import i18n from '../../i18n';
import { describe, expect, it, vi } from 'vitest';
import { runStore } from '../../state/runStore';
import { createMockBackend } from '../../ipc/mock';
import type { Instance } from '../../ipc/schema';
import { zoomAround } from '../../art/bannerLayout';
import { renderApp, sent } from '../../test/backend';

import { renderWithProviders } from '../../test/render';
import { Home } from './Home';

async function openPicker(user: ReturnType<typeof renderApp>['user'], name: string) {
  await screen.findByRole('heading', { name });
  await user.click(screen.getByRole('button', { name: 'More options' }));
  await user.click(screen.getByRole('menuitem', { name: 'Change image…' }));
  return screen.getByRole('dialog', { name: 'Change image' });
}

/** Home's header as the adjust view takes it when it can't be measured. */
const FALLBACK = { width: 1000, height: 562 };

const group = (dialog: HTMLElement, name: 'Icon' | 'Background') => within(within(dialog).getByRole('radiogroup', { name }));
/** Shows one of Background's tabs. */
const showTab = (user: ReturnType<typeof renderApp>['user'], dialog: HTMLElement, name: 'Default' | 'Your screenshots') =>
  user.click(within(dialog).getByRole('tab', { name }));

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

  it("offers a modpack's own art beside Upload, lit while it is in use", async () => {
    const first = renderApp(<Home id="asalith-fields" />);
    const plain = await openPicker(first.user, 'Asalith Fields');
    expect(within(plain).getAllByRole('tab').map((tab) => tab.textContent)).toEqual(['Default', 'Your screenshots']);
    expect(within(plain).getByRole('button', { name: 'Upload your own…' })).toHaveAttribute('aria-pressed', 'false');
    // Not a modpack: nothing of its own to offer.
    expect(within(plain).queryByRole('button', { name: 'From the modpack' })).not.toBeInTheDocument();
    first.unmount();

    const { user, calls } = renderApp(<Home id="sky-islands" />);
    const pack = await openPicker(user, 'Sky Islands');
    const packArt = within(pack).getByRole('button', { name: 'From the modpack' });
    // A modpack starts on its own art.
    expect(packArt).toHaveAttribute('aria-pressed', 'true');
    expect(packArt).toHaveAccessibleDescription('The picture that comes with the modpack.');
    expect(packArt.closest('.art-tabs-row')).not.toBeNull();
    for (const tile of group(pack, 'Background').getAllByRole('radio', { name: /^Landscape \d$/ })) expect(tile).toHaveAttribute('aria-checked', 'false');
    await user.click(group(pack, 'Background').getByRole('radio', { name: 'Landscape 2' }));
    await vi.waitFor(() => expect(within(pack).getByRole('button', { name: 'From the modpack' })).toHaveAttribute('aria-pressed', 'false'));
    // It takes the pack's art back.
    await user.click(within(pack).getByRole('button', { name: 'From the modpack' }));
    expect(sent(calls, 'instance_art_set').at(-1)).toEqual({ id: 'sky-islands', choice: { kind: 'pack' } });
    await vi.waitFor(() => expect(within(pack).getByRole('button', { name: 'From the modpack' })).toHaveAttribute('aria-pressed', 'true'));
  });

  it('uploads a picked image and lights Upload', async () => {
    // The mock's file picker answers an image filter with a picture.
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(within(dialog).getByRole('button', { name: 'Upload your own…' }));
    const [options] = sent(calls, 'plugin:dialog|open') as { options: { filters: { extensions: string[] }[] } }[];
    expect(options?.options.filters[0]?.extensions).toEqual(['png', 'jpg', 'jpeg', 'webp']);
    expect(sent(calls, 'instance_art_set')).toEqual([{ id: 'asalith-fields', choice: { kind: 'upload', path: 'C:/Users/player/Pictures/castle.png' } }]);
    await vi.waitFor(() => expect(within(dialog).getByRole('button', { name: 'Upload your own…' })).toHaveAttribute('aria-pressed', 'true'));
  });

  it("picks one of the instance's screenshots and lights it", async () => {
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    await showTab(user, dialog, 'Your screenshots');
    const background = group(dialog, 'Background');
    // The newest six first; the rest wait behind Show all.
    expect(await background.findAllByRole('radio', { name: /^Screenshot / })).toHaveLength(6);
    await user.click(background.getByRole('button', { name: 'Show all 8' }));
    expect(background.getAllByRole('radio', { name: /^Screenshot / })).toHaveLength(8);
    await user.click(background.getByRole('radio', { name: 'Screenshot 2026-10-16_21.04.11' }));
    expect(sent(calls, 'instance_art_set')).toEqual([{ id: 'asalith-fields', choice: { kind: 'screenshot', name: '2026-10-16_21.04.11.png' } }]);
    expect(await background.findByRole('radio', { name: 'Screenshot 2026-10-16_21.04.11' })).toHaveAttribute('aria-checked', 'true');
    // One lamp: the screenshot, not Upload, though both are a picture of the player's.
    expect(within(dialog).getByRole('button', { name: 'Upload your own…' })).toHaveAttribute('aria-pressed', 'false');
    await showTab(user, dialog, 'Default');
    expect(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 1' })).toHaveAttribute('aria-checked', 'false');
    // A screenshot is a picture like any other, so it can be adjusted.
    expect(within(dialog).getByRole('button', { name: 'Adjust the background' })).toBeInTheDocument();
  });

  it('says where screenshots come from when there are none', async () => {
    const { user } = renderApp(<Home id="sky-islands" />);
    const dialog = await openPicker(user, 'Sky Islands');
    await showTab(user, dialog, 'Your screenshots');
    expect(await within(dialog).findByText('Screenshots you take in the game (F2) show up here.')).toBeInTheDocument();
    expect(group(dialog, 'Background').queryByRole('radio', { name: /^Screenshot / })).not.toBeInTheDocument();
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
    expect(alert).toHaveTextContent('Pick a PNG, JPEG or WebP image.');
    expect(sent(calls, 'instance_art_set')).toHaveLength(1);
    // Nothing changed, so Landscape 1 is still the current one.
    await showTab(user, dialog, 'Default');
    expect(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 1' })).toHaveAttribute('aria-checked', 'true');
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

  it('is the one menu entry for both the icon and the background', async () => {
    const { user } = renderApp(<Home id="asalith-fields" />);
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    await user.click(screen.getByRole('button', { name: 'More options' }));
    const labels = within(screen.getByRole('menu')).getAllByRole('menuitem').map((i) => i.textContent);
    expect(labels.filter((l) => l?.startsWith('Change'))).toEqual(['Change image…']);
  });

  it("can't be opened while the game runs", async () => {
    const { user } = renderApp(<Home id="asalith-fields" />);
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
    await user.click(screen.getByRole('button', { name: 'More options' }));
    expect(screen.getByRole('menuitem', { name: 'Change image…' })).toBeDisabled();
  });

  it('stacks Icon over Background, with a thin rule between them', async () => {
    const { user } = renderApp(<Home id="sky-islands" />);
    const dialog = await openPicker(user, 'Sky Islands');
    const icon = within(dialog).getByRole('radiogroup', { name: 'Icon' });
    const background = within(dialog).getByRole('radiogroup', { name: 'Background' });
    const band = dialog.querySelector('.art-rule')!;
    expect(band).toHaveAttribute('aria-hidden', 'true');
    // In reading order: Icon, the band, then Background.
    expect(icon.compareDocumentPosition(band) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(band.compareDocumentPosition(background) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // A pack's three icon keys share one row.
    const row = icon.querySelector('.art-keys-row')!;
    expect(within(row as HTMLElement).getAllByRole('radio').map((r) => r.textContent)).toEqual([
      expect.stringContaining('Letters'),
      'Pack icon',
      expect.stringContaining('Upload your own…'),
    ]);
  });

  it('moves between Background tabs with the arrow keys, and shows only the one picked', async () => {
    const { user } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    const tabs = within(within(dialog).getByRole('tablist', { name: 'Background' }));
    const landscapes = tabs.getByRole('tab', { name: 'Default' });
    // It opens on Landscapes; one tab stop, the tab shown.
    expect(landscapes).toHaveAttribute('aria-selected', 'true');
    expect(landscapes).toHaveAttribute('tabindex', '0');
    expect(tabs.getByRole('tab', { name: 'Your screenshots' })).toHaveAttribute('tabindex', '-1');
    expect(within(dialog).getByRole('tabpanel', { name: 'Default' })).toContainElement(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 1' }));
    landscapes.focus();
    await user.keyboard('{ArrowRight}');
    const shots = tabs.getByRole('tab', { name: 'Your screenshots' });
    expect(shots).toHaveFocus();
    expect(shots).toHaveAttribute('aria-selected', 'true');
    expect(group(dialog, 'Background').queryByRole('radio', { name: 'Landscape 1' })).not.toBeInTheDocument();
    // Nothing lit among the screenshots: the first keeps the tab stop.
    const tiles = await group(dialog, 'Background').findAllByRole('radio', { name: /^Screenshot / });
    expect(tiles[0]).toHaveAttribute('tabindex', '0');
    await user.keyboard('{ArrowRight}');
    expect(landscapes).toHaveFocus();
  });

  it('keeps Done in view while the dialog scrolls, with Adjust the background beside it', async () => {
    const { user } = renderApp(<Home id="sky-islands" />);
    const dialog = await openPicker(user, 'Sky Islands');
    const foot = within(dialog).getByRole('button', { name: 'Done' }).closest<HTMLElement>('.dialog-actions')!;
    expect(foot).not.toBeNull();
    // Adjust leads the foot, Done ends it.
    expect(within(foot).getAllByRole('button').map((b) => b.textContent)).toEqual(['Adjust the background', 'Done']);
  });

  it('closes with Done', async () => {
    const { user } = renderApp(<Home id="asalith-fields" />);
    const dialog = await openPicker(user, 'Asalith Fields');
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    expect(screen.queryByRole('dialog', { name: 'Change image' })).not.toBeInTheDocument();
  });
});

it.each([
  ['en', 'Upload your own…', 'Upload your own…', 'PNG, JPEG or WebP. Square works best.', 'PNG, JPEG or WebP. Best at 1920 × 1080 or larger.'],
  // "Tuyo" for the icon (el ícono), "tuya" for the background (la imagen).
  ['es', 'Sube el tuyo…', 'Sube la tuya…', 'PNG, JPEG o WebP. Ideal: cuadrada.', 'PNG, JPEG o WebP. Ideal: 1920 × 1080 o más.'],
])('states what an upload may be before picking (%s)', async (lang, iconUpload, upload, iconHint, backgroundHint) => {
  await act(() => i18n.changeLanguage(lang));
  const { user } = renderApp(<Home id="asalith-fields" />);
  await screen.findByRole('heading', { name: 'Asalith Fields' });
  await user.click(screen.getByRole('button', { name: lang === 'en' ? 'More options' : 'Más opciones' }));
  await user.click(screen.getByRole('menuitem', { name: lang === 'en' ? 'Change image…' : 'Cambiar imagen…' }));
  const dialog = screen.getByRole('dialog');
  // Both sections take an upload, and each says what suits it.
  const keys = [within(dialog).getByRole('radio', { name: iconUpload }), within(dialog).getByRole('button', { name: upload })];
  expect(within(dialog).getByRole('radiogroup', { name: lang === 'en' ? 'Icon' : 'Ícono' })).toContainElement(keys[0]!);
  // Background's sits beside its tabs, as an action of its own.
  expect(keys[1]!.closest('.art-tabs-row')).not.toBeNull();
  const tied = (words: string) => words.replaceAll(' ', ' ');
  for (const [key, copy] of [
    [keys[0]!, iconHint],
    [keys[1]!, backgroundHint],
  ] as const) {
    const hint = key.querySelector('.art-key-hint')!.textContent!;
    expect(hint.replaceAll(' ', ' ')).toBe(copy);
    expect(key).toHaveAccessibleDescription(hint);
    // Tied with no-break spaces, so the list of kinds never breaks inside.
    expect(hint).toContain(tied(lang === 'en' ? 'PNG, JPEG or WebP.' : 'PNG, JPEG o WebP.'));
  }
  expect(keys[1]!.querySelector('.art-key-hint')!.textContent).toContain(tied('1920 × 1080'));
});

describe('Change image: adjust the background', () => {
  const adjustKey = (dialog: HTMLElement) => within(dialog).queryByRole('button', { name: 'Adjust the background' });
  const homeBanner = () => document.querySelector<HTMLImageElement>('section[data-home-hero] > .marquee img.marquee-banner');
  const rect = (left: number, top: number, width: number, height: number) => () =>
    ({ left, top, width, height, right: left + width, bottom: top + height, x: left, y: top, toJSON: () => ({}) }) as DOMRect;
  /** The mock's Castle Keep, with its own background picture and, if given, a saved view. */
  function castle(view: Partial<Pick<Instance, 'bannerFocus' | 'bannerZoom' | 'bannerEdges'>> = {}) {
    const backend = createMockBackend({ pictureBackground: true });
    backend.state.listing.instances = backend.state.listing.instances.map((i) => (i.id === 'castle-keep' ? { ...i, ...view } : i));
    const calls: [string, unknown][] = [];
    const rendered = renderWithProviders(<Home id="castle-keep" />, {
      ipc: (cmd, args) => {
        calls.push([cmd, args]);
        return backend.handle(cmd, args);
      },
    });
    return { ...rendered, backend, calls };
  }
  const saved = (calls: [string, unknown][]) => sent(calls, 'instance_banner_view_set');
  /** Open Change image, then Adjust…; the same dialog now shows the adjust view. */
  async function adjust(user: ReturnType<typeof renderApp>['user']) {
    const picker = await openPicker(user, 'Castle Keep');
    await user.click(await within(picker).findByRole('button', { name: 'Adjust the background' }));
    return screen.getByRole('dialog', { name: 'Adjust the background' });
  }

  it('offers Adjust only while a picture is the background', async () => {
    const plain = renderApp(<Home id="asalith-fields" />, { pictureBackground: true });
    expect(adjustKey(await openPicker(plain.user, 'Asalith Fields'))).toBeNull();
    plain.unmount();

    const pack = renderApp(<Home id="sky-islands" />, { pictureBackground: true });
    expect(await within(await openPicker(pack.user, 'Sky Islands')).findByRole('button', { name: 'Adjust the background' })).toBeInTheDocument();
    pack.unmount();

    const { user } = castle();
    const picker = await openPicker(user, 'Castle Keep');
    expect(await within(picker).findByRole('button', { name: 'Adjust the background' })).toBeInTheDocument();
    // Position no longer lives in Change image.
    expect(within(picker).queryByRole('group', { name: 'Position' })).toBeNull();
  });

  it('switches the same dialog to the adjust view, with Back and Done', async () => {
    const { user } = castle();
    const view = await adjust(user);
    expect(view).toHaveAccessibleDescription('Drag the picture to move it and use the zoom to bring it closer or further away.');
    // The preview is Home's own header, for the eye only.
    const preview = view.querySelector<HTMLElement>('.banner-preview')!;
    expect(preview).toHaveAttribute('aria-hidden', 'true');
    expect(preview.hasAttribute('inert')).toBe(true);
    expect(preview.querySelector('section')).not.toBeNull();
    await user.click(within(view).getByRole('button', { name: 'Back' }));
    expect(screen.getByRole('dialog', { name: 'Change image' })).toBeInTheDocument();
    await user.click(await within(screen.getByRole('dialog')).findByRole('button', { name: 'Adjust the background' }));
    await user.click(within(screen.getByRole('dialog', { name: 'Adjust the background' })).getByRole('button', { name: 'Done' }));
    await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('moves by 5 with the arrow keys and saves once, 300 ms after the last press', async () => {
    const { user, calls } = castle();
    const view = await adjust(user);
    const x = within(view).getByLabelText('Left to right');
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      x.focus();
      fireEvent.keyDown(x, { key: 'ArrowLeft' });
      fireEvent.keyDown(x, { key: 'ArrowUp' });
      expect(x).toHaveValue('45');
      expect(within(view).getByLabelText('Top to bottom')).toHaveValue('95');
      act(() => vi.advanceTimersByTime(299));
      // The marquee behind the dialog follows before the save.
      expect(homeBanner()?.style.objectPosition).toBe('45% 95%');
      expect(saved(calls)).toEqual([]);
      act(() => vi.advanceTimersByTime(1));
      await vi.waitFor(() => expect(saved(calls)).toEqual([{ id: 'castle-keep', view: { focus: { x: 45, y: 95 }, zoom: null, edges: null } }]));
    } finally {
      vi.useRealTimers();
    }
  });

  it('zooms with the slider, the keys beside it and the wheel, saving 300 ms after the last step', async () => {
    const { user, calls } = castle();
    const view = await adjust(user);
    const zoom = within(view).getByRole('slider', { name: 'Zoom' });
    expect(zoom).toHaveAttribute('min', '25');
    expect(zoom).toHaveAttribute('max', '500');
    expect(zoom).toHaveAttribute('step', '5');
    expect(zoom).toHaveValue('100');
    expect(within(view).getByText('100 %')).toBeInTheDocument();
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      fireEvent.click(within(view).getByRole('button', { name: 'Zoom out' }));
      expect(zoom).toHaveValue('95');
      fireEvent.change(zoom, { target: { value: '60' } });
      fireEvent.wheel(view.querySelector('.banner-preview-drag')!, { deltaY: -100 });
      expect(zoom).toHaveValue('65');
      act(() => vi.advanceTimersByTime(300));
      await vi.waitFor(() => expect(saved(calls)).toEqual([{ id: 'castle-keep', view: { focus: null, zoom: 65, edges: null } }]));
    } finally {
      vi.useRealTimers();
    }
  });

  it('starts the zoom from what Home shows, so the first step never jumps', async () => {
    const { user, calls } = castle();
    const view = await adjust(user);
    // A 500 × 500 picture covering the 1000 × 562 header is 1000 tall: 178 % of it, on the step 180.
    const probe = view.querySelector<HTMLImageElement>('img.banner-probe')!;
    Object.defineProperty(probe, 'naturalWidth', { value: 500 });
    Object.defineProperty(probe, 'naturalHeight', { value: 500 });
    fireEvent.load(probe);
    const zoom = within(view).getByRole('slider', { name: 'Zoom' });
    expect(zoom).toHaveValue('180');
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      fireEvent.click(within(view).getByRole('button', { name: 'Zoom out' }));
      act(() => vi.advanceTimersByTime(300));
    } finally {
      vi.useRealTimers();
    }
    // The position becomes the one that keeps the picture where it was: the bottom middle.
    await vi.waitFor(() => expect(saved(calls)).toEqual([{ id: 'castle-keep', view: { focus: { x: 50, y: 100 }, zoom: 175, edges: null } }]));
  });

  /** The 500 × 500 picture's own size, as the probe reports it once loaded. */
  function loadSquare(view: HTMLElement) {
    const probe = view.querySelector<HTMLImageElement>('img.banner-probe')!;
    Object.defineProperty(probe, 'naturalWidth', { value: 500 });
    Object.defineProperty(probe, 'naturalHeight', { value: 500 });
    fireEvent.load(probe);
  }
  const square = { width: 500, height: 500 };

  it('zooms the keys and the slider into the middle of what shows', async () => {
    const { user, calls } = castle({ bannerFocus: { x: 30, y: 40 }, bannerZoom: 200 });
    const view = await adjust(user);
    loadSquare(view);
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      fireEvent.click(within(view).getByRole('button', { name: 'Zoom in' }));
      act(() => vi.advanceTimersByTime(300));
    } finally {
      vi.useRealTimers();
    }
    const focus = zoomAround(FALLBACK, square, 200, { x: 30, y: 40 }, 205);
    // Not where it was: a zoom that kept x% and y% would close in on a corner.
    expect(focus).not.toEqual({ x: 30, y: 40 });
    await vi.waitFor(() => expect(saved(calls)).toEqual([{ id: 'castle-keep', view: { focus, zoom: 205, edges: null } }]));
  });

  it('zooms the wheel into the point under the pointer', async () => {
    const { user, calls } = castle({ bannerFocus: { x: 50, y: 50 }, bannerZoom: 200 });
    const view = await adjust(user);
    loadSquare(view);
    const drag = view.querySelector<HTMLElement>('.banner-preview-drag')!;
    // The header shows at half size: the pointer at (300, 150) is the header's (600, 300).
    drag.getBoundingClientRect = rect(0, 0, 500, 281);
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      fireEvent.wheel(drag, { deltaY: -100, clientX: 300, clientY: 150 });
      act(() => vi.advanceTimersByTime(300));
    } finally {
      vi.useRealTimers();
    }
    const focus = zoomAround(FALLBACK, square, 200, { x: 50, y: 50 }, 205, { x: 600, y: 300 });
    expect(focus).not.toEqual(zoomAround(FALLBACK, square, 200, { x: 50, y: 50 }, 205));
    await vi.waitFor(() => expect(saved(calls)).toEqual([{ id: 'castle-keep', view: { focus, zoom: 205, edges: null } }]));
  });

  it('shows Edges only while the picture no longer covers the header, and saves a pick at once', async () => {
    const { user, calls } = castle();
    const view = await adjust(user);
    const edges = () => view.querySelector<HTMLElement>('.banner-edges-row')!;
    // Hidden but keeping its room, so the view doesn't jump.
    expect(edges().style.visibility).toBe('hidden');
    fireEvent.change(within(view).getByRole('slider', { name: 'Zoom' }), { target: { value: '60' } });
    expect(edges().style.visibility).toBe('');
    expect(within(view).getByRole('radio', { name: 'Dark' })).toBeChecked();
    await user.click(within(view).getByRole('radio', { name: 'Pixel' }));
    await vi.waitFor(() => expect(saved(calls).at(-1)).toEqual({ id: 'castle-keep', view: { focus: null, zoom: 60, edges: 'pixel' } }));
  });

  it('saves a drag on the preview when the pointer is released', async () => {
    const { user, calls } = castle();
    const view = await adjust(user);
    // The picture is 500 × 500; the header (1000 × 562 when it can't be measured) shows at half size.
    const probe = view.querySelector<HTMLImageElement>('img.banner-probe')!;
    Object.defineProperty(probe, 'naturalWidth', { value: 500 });
    Object.defineProperty(probe, 'naturalHeight', { value: 500 });
    fireEvent.load(probe);
    const drag = view.querySelector<HTMLElement>('.banner-preview-drag')!;
    drag.getBoundingClientRect = rect(0, 0, 500, 281);
    fireEvent.pointerDown(drag, { clientX: 100, clientY: 100, pointerId: 1, button: 0 });
    fireEvent.pointerMove(drag, { clientX: 100, clientY: 150, pointerId: 1 });
    expect(saved(calls)).toEqual([]);
    fireEvent.pointerUp(drag, { clientX: 100, clientY: 150, pointerId: 1 });
    // Covering 1000 wide, the picture can't move sideways; dragging it down 100 header pixels shows more of its top.
    await vi.waitFor(() => expect(saved(calls)).toEqual([{ id: 'castle-keep', view: { focus: { x: 50, y: 77 }, zoom: null, edges: null } }]));
  });

  it('goes back to the default with Reset', async () => {
    const { user, calls } = castle({ bannerFocus: { x: 30, y: 60 }, bannerZoom: 60, bannerEdges: 'pixel' });
    const view = await adjust(user);
    expect(within(view).getByRole('slider', { name: 'Zoom' })).toHaveValue('60');
    await user.click(within(view).getByRole('button', { name: 'Reset' }));
    expect(saved(calls)).toEqual([{ id: 'castle-keep', view: { focus: null, zoom: null, edges: null } }]);
    expect(within(view).getByRole('slider', { name: 'Zoom' })).toHaveValue('100');
    // Already the default: nothing more to send.
    expect(within(view).getByRole('button', { name: 'Reset' })).toHaveAttribute('aria-disabled', 'true');
    await user.click(within(view).getByRole('button', { name: 'Reset' }));
    expect(saved(calls)).toHaveLength(1);
  });

  it('saves what is still waiting when the dialog closes or goes Back', async () => {
    for (const close of ['Done', 'Back', 'Escape'] as const) {
      const { user, calls, unmount } = castle();
      const view = await adjust(user);
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
      try {
        fireEvent.keyDown(within(view).getByLabelText('Left to right'), { key: 'ArrowRight' });
        // jsdom doesn't turn Escape into the dialog's cancel event; dispatch it as the browser would.
        if (close === 'Escape') act(() => void view.dispatchEvent(new Event('cancel', { cancelable: true })));
        else fireEvent.click(within(view).getByRole('button', { name: close }));
      } finally {
        // The 300 ms wait never ends: only leaving can have sent it.
        vi.useRealTimers();
      }
      await vi.waitFor(() => expect(saved(calls)).toEqual([{ id: 'castle-keep', view: { focus: { x: 55, y: 100 }, zoom: null, edges: null } }]));
      unmount();
    }
  });

  it('starts a new picture from the default view', async () => {
    const { user } = castle({ bannerFocus: { x: 30, y: 60 }, bannerZoom: 60 });
    const picker = await openPicker(user, 'Castle Keep');
    await user.click(within(picker).getByRole('button', { name: 'Upload your own…' }));
    await user.click(await within(picker).findByRole('button', { name: 'Adjust the background' }));
    const view = screen.getByRole('dialog', { name: 'Adjust the background' });
    await vi.waitFor(() => expect(within(view).getByRole('slider', { name: 'Zoom' })).toHaveValue('100'));
    expect(within(view).getByLabelText('Left to right')).toHaveValue('50');
  });

  it('shows a refused view as a translated error and puts the marquee back', async () => {
    const { user, backend } = castle();
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
      cmd === 'instance_banner_view_set'
        ? Promise.reject({ category: 'disk', code: 'banner_zoom_invalid', message: 'That zoom is out of range', fix: null, detail: null, params: {} })
        : handle(cmd, args),
    );
    const view = await adjust(user);
    await user.click(within(view).getByRole('button', { name: 'Zoom in' }));
    expect(await within(view).findByRole('alert', {}, { timeout: 2000 })).toHaveTextContent('That zoom is out of range.');
    await vi.waitFor(() => expect(within(view).getByRole('slider', { name: 'Zoom' })).toHaveValue('100'));
  });
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
    expect(icons[0]).toHaveAccessibleName('Letters');
    // The second line says where the letters come from.
    expect(icons[0]).toHaveAccessibleDescription("Made from the instance's name.");
    expect(icons[1]).toHaveAccessibleName('Upload your own…');
    // The landscapes are backgrounds only.
    expect(group(dialog, 'Icon').queryByRole('radio', { name: /^Landscape/ })).not.toBeInTheDocument();
  });

  it('sends the initials as the generated icon, leaving the background alone', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    const dialog = await openPicker(user, 'Sky Islands');
    const icons = group(dialog, 'Icon');
    expect(icons.getByRole('radio', { name: 'Pack icon' })).toHaveAttribute('aria-checked', 'true');
    await user.click(icons.getByRole('radio', { name: 'Letters' }));
    expect(sent(calls, 'instance_icon_set')).toEqual([{ id: 'sky-islands', choice: { kind: 'generated' } }]);
    expect(sent(calls, 'instance_art_set')).toEqual([]);
    expect(await group(dialog, 'Icon').findByRole('radio', { name: 'Letters' })).toHaveAttribute('aria-checked', 'true');
    expect(group(dialog, 'Icon').getByRole('radio', { name: 'Pack icon' })).toHaveAttribute('aria-checked', 'false');
    // The background is still the pack's art.
    expect(within(dialog).getByRole('button', { name: 'Upload your own…' })).toHaveAttribute('aria-pressed', 'false');
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
    expect(within(dialog).getByRole('button', { name: 'Upload your own…' })).toHaveAttribute('aria-pressed', 'false');
  });

  it.each<[string, Partial<Instance>, string, string]>([
    ['a never-chosen icon on a plain instance', { icon: null, art: 'auto' }, 'asalith-fields', 'Letters'],
    ['a never-chosen icon on a pack', { icon: null, art: 'auto' }, 'sky-islands', 'Pack icon'],
    ['a round-1 upload, which is still the icon', { icon: null, art: 'custom' }, 'asalith-fields', 'Upload your own…'],
    ['the initials on a pack', { icon: 'generated' }, 'sky-islands', 'Letters'],
    ['auto on a plain instance', { icon: 'auto', art: 'generated' }, 'asalith-fields', 'Letters'],
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

  it('lights the initials for a pack with no icon of its own, and still offers the pack icon', async () => {
    const backend = createMockBackend();
    backend.state.packs.get('sky-islands')!.art = { icon: null, banner: null };
    const { user } = renderWithProviders(<Home id="sky-islands" />, { ipc: (cmd, args) => backend.handle(cmd, args) });
    const dialog = await openPicker(user, 'Sky Islands');
    const icons = group(dialog, 'Icon');
    expect(await icons.findByRole('radio', { name: 'Letters' })).toHaveAttribute('aria-checked', 'true');
    expect(icons.getByRole('radio', { name: 'Pack icon' })).toHaveAttribute('aria-checked', 'false');
    expect(icons.getAllByRole('radio').filter((r) => r.tabIndex === 0)).toEqual([icons.getByRole('radio', { name: 'Letters' })]);
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
    expect(group(dialog, 'Icon').getByRole('radio', { name: 'Letters' })).toHaveAttribute('aria-checked', 'true');
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
    await showTab(user, dialog, 'Default');
    await user.click(group(dialog, 'Icon').getByRole('radio', { name: 'Letters' }));
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
    await showTab(user, dialog, 'Default');
    await user.click(group(dialog, 'Background').getByRole('radio', { name: 'Landscape 2' }));
    await user.click(group(dialog, 'Icon').getByRole('radio', { name: 'Letters' }));
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
    expect(icons.getByRole('radio', { name: 'Letters' })).toHaveFocus();
    // Moving never saves: an upload key would open the file picker.
    expect(sent(calls, 'instance_icon_set')).toEqual([]);
    await user.keyboard(' ');
    expect(sent(calls, 'instance_icon_set')).toEqual([{ id: 'sky-islands', choice: { kind: 'generated' } }]);
  });
});
