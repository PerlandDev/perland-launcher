import { screen, waitFor, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { toastsSnapshot } from '../../state/toasts';
import { resetAppearance } from '../../state/appearance';
import { navigate, routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { Personalize } from './Personalize';
import { TitleBar } from '../titlebar/TitleBar';
import { Settings } from '../settings/Settings';
import { BACKGROUNDS, BUTTONS, buttonsFor } from '../../state/customRoom';
import { afterEach } from 'vitest';

afterEach(resetAppearance);

const open = () => renderApp(<Personalize />);

it('offers every theme, the new ones tagged, and puts one on at once', async () => {
  const { user, calls } = open();
  const rooms = await screen.findByRole('radiogroup', { name: 'Themes' });
  for (const name of ['Laminate', 'Stone', 'Night', 'Forest']) expect(within(rooms).getByRole('radio', { name })).toBeInTheDocument();
  for (const name of ['Nether', 'End', 'Cherry']) expect(within(rooms).getByRole('radio', { name: `${name} New` })).toBeInTheDocument();
  expect(within(rooms).getByRole('radio', { name: 'Laminate' })).toBeChecked();

  await user.click(within(rooms).getByRole('radio', { name: 'Cherry New' }));
  expect(document.documentElement).toHaveAttribute('data-theme', 'cherry');
  await waitFor(() => expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { theme: 'cherry' } }));
});

it('picks a button colour and saves it', async () => {
  const { user, calls } = open();
  const accents = await screen.findByRole('radiogroup', { name: 'Button colour' });
  expect(within(accents).getByRole('radio', { name: "The theme's" })).toBeChecked();
  await user.click(within(accents).getByRole('radio', { name: 'Violet' }));
  expect(document.documentElement).toHaveAttribute('data-accent', 'violet');
  await waitFor(() => expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { accent: 'violet' } }));
});

it('offers your colours only on your own room, and the button colour only off it', async () => {
  const { user } = open();
  const rooms = await screen.findByRole('radiogroup', { name: 'Themes' });
  expect(screen.queryByRole('region', { name: 'Your colours' })).toBeNull();

  // Your own theme is not one of the made ones: a button below them makes it.
  expect(within(rooms).queryByRole('radio', { name: /Your/ })).toBeNull();
  const customize = screen.getByRole('button', { name: /^Customize theme/ });
  expect(customize).toHaveAttribute('aria-pressed', 'false');
  await user.click(customize);
  expect(customize).toHaveAttribute('aria-pressed', 'true');
  expect(document.documentElement).toHaveAttribute('data-theme', 'custom');
  expect(await screen.findByRole('region', { name: 'Your colours' })).toBeInTheDocument();
  expect(screen.queryByRole('radiogroup', { name: 'Button colour' })).toBeNull();

  // Picking a made theme again leaves your own.
  await user.click(within(rooms).getByRole('radio', { name: 'Stone' }));
  expect(customize).toHaveAttribute('aria-pressed', 'false');
  expect(screen.queryByRole('region', { name: 'Your colours' })).toBeNull();
});

it('makes your theme from a background and buttons picked from palettes, and saves them', async () => {
  const { user, calls } = open();
  await user.click(await screen.findByRole('button', { name: /^Customize theme/ }));
  const own = await screen.findByRole('region', { name: 'Your colours' });
  const backgrounds = within(own).getByRole('radiogroup', { name: 'Background' });
  const buttons = within(own).getByRole('radiogroup', { name: 'Buttons' });
  // The room starts from the default, which both palettes hold.
  expect(within(backgrounds).getByRole('radio', { name: 'Slate, dark' })).toBeChecked();
  expect(within(buttons).getByRole('radio', { name: 'Magenta, bold' })).toBeChecked();
  // No free colour field: only colours that read are offered.
  expect(within(own).queryByRole('textbox')).toBeNull();

  const lava = BACKGROUNDS.find((s) => s.hue === 'orange' && s.tone === 'veryDark')!;
  await user.click(within(backgrounds).getByRole('radio', { name: 'Orange, very dark' }));
  await waitFor(() => expect(document.documentElement.style.getPropertyValue('--color-ground')).toBe(lava.value));
  const sky = BUTTONS.find((s) => s.hue === 'sky' && s.tone === 'light')!;
  await user.click(within(buttons).getByRole('radio', { name: 'Sky, light' }));
  expect(document.documentElement).toHaveAttribute('data-theme', 'custom');
  await waitFor(() => expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { theme: 'custom', customRoom: { background: lava.value, buttons: sky.value } } }));
});

it('offers only the buttons that read on the chosen background', async () => {
  const { user } = open();
  await user.click(await screen.findByRole('button', { name: /^Customize theme/ }));
  const own = await screen.findByRole('region', { name: 'Your colours' });
  for (const background of [BACKGROUNDS[0]!, BACKGROUNDS.at(-1)!]) {
    await user.click(within(within(own).getByRole('radiogroup', { name: 'Background' })).getAllByRole('radio')[BACKGROUNDS.indexOf(background)]!);
    const offered = within(within(own).getByRole('radiogroup', { name: 'Buttons' })).getAllByRole('radio');
    expect(offered).toHaveLength(buttonsFor(background.value).length);
    // One of them is always chosen: picking a background never leaves buttons that would not read.
    expect(offered.filter((radio) => (radio as HTMLInputElement).checked)).toHaveLength(1);
  }
});

it('keeps the text font, instance icons and transitions', async () => {
  const { user, calls } = open();
  const font = await screen.findByRole('group', { name: 'Text font' });
  expect(within(font).getByRole('radio', { name: 'Easy to read' })).toBeChecked();
  await user.click(within(font).getByRole('radio', { name: "This computer's" }));
  expect(document.documentElement).toHaveAttribute('data-font', 'system');
  await waitFor(() => expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { font: 'system' } }));

  const icons = screen.getByRole('group', { name: 'Instance icons' });
  expect(within(icons).getByRole('radio', { name: 'Square' })).toBeChecked();
  await user.click(within(icons).getByRole('radio', { name: 'Round' }));
  await waitFor(() => expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { iconShape: 'round' } }));

  const screens = screen.getByRole('group', { name: 'Screen transitions' });
  expect(within(screens).getAllByRole('radio').map((r) => r.parentElement?.textContent)).toEqual(['Smooth', 'Arcade', 'Off']);
  await user.click(within(screens).getByRole('radio', { name: 'Arcade' }));
  await waitFor(() => expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { transitions: 'stepped' } }));
});

it('puts the stored room back when saving it fails', async () => {
  const { user, backend } = open();
  const handle = backend.handle;
  const failure = { category: 'game', code: 'internal', message: 'nope', fix: null, detail: null, params: {} };
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'settings_set' ? Promise.reject(failure) : handle(cmd, args)));
  await user.click(within(await screen.findByRole('radiogroup', { name: 'Themes' })).getByRole('radio', { name: 'Nether New' }));
  await waitFor(() => expect(toastsSnapshot()).toHaveLength(1));
  await waitFor(() => expect(document.documentElement).toHaveAttribute('data-theme', 'laminate'));
  expect(screen.getByRole('radio', { name: 'Laminate' })).toBeChecked();
});

it('is opened by the brush in the title bar', async () => {
  const { user } = renderApp(<TitleBar />);
  const brush = await screen.findByRole('button', { name: 'Personalize' });
  expect(brush).not.toHaveAttribute('aria-current');
  await user.click(brush);
  expect(routeSnapshot()).toEqual({ name: 'personalize' });
  expect(await screen.findByRole('button', { name: 'Personalize' })).toHaveAttribute('aria-current', 'page');
});

it('is not in Settings: the brush is the only way in', async () => {
  navigate({ name: 'settings' });
  renderApp(<Settings />);
  await screen.findByRole('heading', { name: 'Launcher updates' });
  expect(screen.queryByRole('region', { name: 'Appearance' })).toBeNull();
  expect(screen.queryByRole('button', { name: 'Open Personalize' })).toBeNull();
});
