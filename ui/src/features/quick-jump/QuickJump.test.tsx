import { act, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import App from '../../App';
import i18n from '../../i18n';
import { routeSnapshot } from '../../state/route';
import { renderApp } from '../../test/backend';

const palette = () => screen.getByRole('dialog', { name: /^(Go to|Ir a)$/, hidden: true });
const input = () => within(palette()).getByRole('combobox', { hidden: true });
const options = () => within(palette()).queryAllByRole('option', { hidden: true });
const active = () => document.getElementById(input().getAttribute('aria-activedescendant') ?? '');
const isOpen = () => screen.queryByRole('dialog', { name: 'Go to', hidden: true })?.hasAttribute('open') ?? false;

async function shell() {
  const rendered = renderApp(<App />);
  await screen.findByRole('heading', { name: 'Asalith Fields' });
  return rendered;
}

afterEach(() => void i18n.changeLanguage('en'));

describe('Go to', () => {
  it('opens with Ctrl+K from anywhere in the shell, with the keyboard in its search', async () => {
    const { user } = await shell();
    expect(isOpen()).toBe(false);
    await user.keyboard('{Control>}k{/Control}');
    expect(isOpen()).toBe(true);
    expect(input()).toHaveFocus();
    expect(input()).toHaveAttribute('aria-controls', within(palette()).getByRole('listbox', { hidden: true }).id);
  });

  it("opens from the title bar's search key, whose title names the shortcut", async () => {
    const { user } = await shell();
    const key = screen.getByRole('button', { name: 'Search' });
    expect(key).toHaveAttribute('title', 'Search (Ctrl+K)');
    expect(key).toHaveAttribute('aria-keyshortcuts', 'Control+K');
    await user.click(key);
    expect(isOpen()).toBe(true);
    expect(input()).toHaveFocus();
  });

  it('stays shut while another dialog is open', async () => {
    const { user } = await shell();
    const other = document.createElement('dialog');
    document.body.append(other);
    other.showModal();
    await user.keyboard('{Control>}k{/Control}');
    expect(isOpen()).toBe(false);
    other.remove();
  });

  it('is not there during first run', async () => {
    const { user } = renderApp(<App />, { firstRun: true });
    await screen.findByRole('heading', { name: 'Welcome to Perland' });
    await user.keyboard('{Control>}k{/Control}');
    expect(screen.queryByRole('dialog', { name: 'Go to', hidden: true })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Search' })).toBeNull();
  });

  it("suggests the current instance's screens, Settings and Personalize before anything is typed", async () => {
    const { user } = await shell();
    await user.keyboard('{Control>}k{/Control}');
    expect(options().map((o) => o.textContent)).toEqual(['Home', 'Mods & packs', 'Resource packs', 'Shaders', 'Instance settings', 'Personalize', 'Settings']);
    expect(within(palette()).getByRole('group', { name: 'Asalith Fields', hidden: true })).toBeInTheDocument();
  });

  it('moves with the arrow keys, wrapping at the ends, and opens with Enter', async () => {
    const { user } = await shell();
    await user.keyboard('{Control>}k{/Control}');
    expect(active()).toHaveTextContent('Home');
    expect(active()).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{ArrowDown}{ArrowDown}');
    expect(active()).toHaveTextContent('Resource packs');
    expect(options().filter((o) => o.getAttribute('aria-selected') === 'true')).toHaveLength(1);
    await user.keyboard('{ArrowUp}{ArrowUp}{ArrowUp}');
    expect(active()).toHaveTextContent('Settings');
    await user.keyboard('{ArrowDown}{ArrowDown}{ArrowDown}');
    expect(active()).toHaveTextContent('Resource packs');
    await user.keyboard('{Enter}');
    expect(routeSnapshot()).toEqual({ name: 'mods', id: 'asalith-fields', tab: 'resource_pack' });
    expect(isOpen()).toBe(false);
  });

  it('closes with Escape and gives the keyboard back to where it was', async () => {
    const { user } = await shell();
    const key = screen.getByRole('button', { name: 'Search' });
    await user.click(key);
    await user.keyboard('sk');
    await user.keyboard('{Escape}');
    expect(isOpen()).toBe(false);
    expect(key).toHaveFocus();
    // Opened again, it starts empty.
    await user.keyboard('{Control>}k{/Control}');
    expect(input()).toHaveValue('');
    const settings = screen.getByRole('button', { name: 'Settings' });
    await user.keyboard('{Escape}');
    settings.focus();
    await user.keyboard('{Control>}k{/Control}{Escape}');
    expect(settings).toHaveFocus();
  });

  it('matches whatever the accents, in the language in use', async () => {
    const { user } = await shell();
    await act(() => i18n.changeLanguage('es'));
    await user.keyboard('{Control>}k{/Control}');
    await user.keyboard('configuracion');
    expect(options()[0]).toHaveTextContent('Configuración');
    await user.clear(input());
    await user.keyboard('iconos');
    expect(options().map((o) => o.textContent)).toContain('Íconos de instancias');
    await user.clear(input());
    await user.keyboard('zzzz');
    expect(options()).toHaveLength(0);
    expect(within(palette()).getByText('No hay nada llamado “zzzz”.')).toBeInTheDocument();
  });

  it('opens an instance screen, the current instance first', async () => {
    const { user } = await shell();
    await user.keyboard('{Control>}k{/Control}');
    await user.keyboard('shaders');
    expect(active()).toHaveTextContent('Shaders');
    await user.keyboard('{Enter}');
    expect(routeSnapshot()).toEqual({ name: 'mods', id: 'asalith-fields', tab: 'shader' });
  });

  it("opens another instance's screen by its name", async () => {
    const { user } = await shell();
    await user.keyboard('{Control>}k{/Control}');
    await user.keyboard('creative settings');
    expect(active()).toHaveTextContent('Instance settings');
    await user.click(active()!);
    expect(routeSnapshot()).toEqual({ name: 'instanceSettings', id: 'creative-flat' });
  });

  it('opens Settings on the field asked for, with the keyboard on it', async () => {
    const { user } = await shell();
    await user.keyboard('{Control>}k{/Control}');
    await user.keyboard('language');
    expect(active()).toHaveTextContent('Language');
    await user.keyboard('{Enter}');
    expect(routeSnapshot()).toEqual({ name: 'settings' });
    await waitFor(() => expect(screen.getByRole('combobox', { name: 'Language' })).toHaveFocus());
  });

  it('runs an instance action, opening its dialog', async () => {
    const { user } = await shell();
    await user.keyboard('{Control>}k{/Control}');
    await user.keyboard('rename');
    expect(active()).toHaveTextContent('Rename');
    await user.keyboard('{Enter}');
    expect(await screen.findByRole('dialog', { name: 'Rename instance', hidden: true })).toHaveAttribute('open');
    expect(isOpen()).toBe(false);
    expect(screen.getByRole('textbox', { name: 'Name', hidden: true })).toHaveValue('Asalith Fields');
  });

  it('lands on the title of a place it opens', async () => {
    const { user } = await shell();
    await user.keyboard('{Control>}k{/Control}');
    await user.keyboard('servers');
    await user.keyboard('{Enter}');
    expect(routeSnapshot()).toEqual({ name: 'servers' });
    await waitFor(() => expect(screen.getByRole('heading', { name: 'Servers', level: 1 })).toHaveFocus());
  });
});
