import { screen, waitFor, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { toastsSnapshot } from '../../state/toasts';
import { resetAppearance } from '../../state/appearance';
import { navigate, routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { Personalize } from './Personalize';
import { TitleBar } from '../titlebar/TitleBar';
import { Settings } from '../settings/Settings';
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

it('mixes your room from two colours and saves them', async () => {
  const { user, calls } = open();
  await user.click(await screen.findByRole('button', { name: /^Customize theme/ }));
  const own = await screen.findByRole('region', { name: 'Your colours' });

  const code = within(own).getByRole('textbox', { name: 'Colour code: Background' });
  await user.clear(code);
  await user.type(code, '#1c0f0c');
  await user.tab();

  await waitFor(() => expect(document.documentElement.style.getPropertyValue('--color-ground')).toBe('#1c0f0c'));
  expect(document.documentElement).toHaveAttribute('data-theme', 'custom');
  await waitFor(() => expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { theme: 'custom', customRoom: { background: '#1c0f0c' } } }));
});

it('says what a room would break and leaves it off the page', async () => {
  const { user } = open();
  await user.click(await screen.findByRole('button', { name: /^Customize theme/ }));
  const own = await screen.findByRole('region', { name: 'Your colours' });
  expect(within(own).getByText('The background is dark, as the launcher needs.')).toBeInTheDocument();

  const code = within(own).getByRole('textbox', { name: 'Colour code: Background' });
  await user.clear(code);
  await user.type(code, '#d8d0c0');
  await user.tab();

  expect(await within(own).findByText('The background is too light. Pick a darker one.')).toBeInTheDocument();
  expect(within(own).getByText("Play's words wouldn't read: try lighter buttons.")).toBeInTheDocument();
  // Kept, but never put on: the launcher shows the default room meanwhile.
  await waitFor(() => expect(document.documentElement).toHaveAttribute('data-theme', 'laminate'));
  expect(document.documentElement.style.getPropertyValue('--color-ground')).toBe('');
});

it('starts your room from an idea', async () => {
  const { user } = open();
  await user.click(await screen.findByRole('button', { name: /^Customize theme/ }));
  const own = await screen.findByRole('region', { name: 'Your colours' });
  await user.click(within(own).getByRole('button', { name: 'Lava' }));
  await waitFor(() => expect(document.documentElement.style.getPropertyValue('--color-ground')).toBe('#1c0f0c'));
  expect(within(own).getByRole('textbox', { name: 'Colour code: Buttons' })).toHaveValue('#ff7a2f');
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
