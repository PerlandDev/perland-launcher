import { screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import i18n from '../../i18n';
import { renderApp } from '../../test/backend';
import { Personalize } from '../personalize/Personalize';
import { Settings } from '../settings/Settings';
import { buildEntries } from './registry';
import { land, reveal } from './reveal';

const t = i18n.t.bind(i18n);
const fieldsOf = (group: string, ownTheme = false) =>
  buildEntries({ t, instances: [], current: null, running: [], ownTheme }).filter((e) => e.group.id === group && e.target.kind === 'field');
const findOf = (label: string, group: string) => {
  const target = fieldsOf(group).find((e) => e.label === label)!.target;
  return target.kind === 'field' ? target.find : '';
};

describe('reveal', () => {
  it("focuses Settings' fields by the labels the screen shows", async () => {
    renderApp(
      <main>
        <Settings />
      </main>,
    );
    await screen.findByRole('combobox', { name: 'Language' });
    expect(await reveal(findOf('Language', 'settings'))).toBe(true);
    expect(document.activeElement).toBe(screen.getByRole('combobox', { name: 'Language' }));
    expect(await reveal(findOf('Memory for new instances', 'settings'))).toBe(true);
    expect(document.activeElement).toBe(screen.getByRole('slider', { name: 'Memory for new instances' }));
    expect(await reveal(findOf('Minimize while playing', 'settings'))).toBe(true);
    expect(document.activeElement).toBe(screen.getByRole('switch', { name: 'Minimize while playing' }));
    expect(await reveal(findOf('Data folder', 'settings'))).toBe(true);
    expect(document.activeElement).toHaveTextContent('Open folder');
    expect(await reveal(findOf('Privacy', 'settings'))).toBe(true);
    expect(within(screen.getByRole('region', { name: 'Privacy' })).getAllByRole('button')).toContain(document.activeElement);
    expect(await reveal(findOf('About', 'settings'))).toBe(true);
    expect(within(screen.getByRole('region', { name: 'Perland Launcher' })).getAllByRole('button')).toContain(document.activeElement);
  });

  it('finds every field Go to lists for Settings', async () => {
    renderApp(
      <main>
        <Settings />
      </main>,
    );
    await screen.findByRole('combobox', { name: 'Language' });
    for (const entry of fieldsOf('settings')) {
      if (entry.target.kind !== 'field') continue;
      expect(await reveal(entry.target.find, 200), entry.label).toBe(true);
      expect(document.activeElement, entry.label).not.toBe(document.body);
    }
  });

  it('finds every field Go to lists for Personalize, with a made theme and with the own one', async () => {
    const { user } = renderApp(
      <main>
        <Personalize />
      </main>,
    );
    await screen.findByRole('radio', { name: 'Laminate' });
    for (const entry of fieldsOf('personalize')) {
      if (entry.target.kind !== 'field') continue;
      expect(await reveal(entry.target.find, 200), entry.label).toBe(true);
      expect(document.activeElement, entry.label).not.toBe(document.body);
    }
    // A choice lands on the option that is chosen, so the arrow keys go on from there.
    await reveal(findOf('Themes', 'personalize'));
    expect(document.activeElement).toBe(screen.getByRole('radio', { name: 'Laminate' }));
    await reveal(findOf('Customize theme', 'personalize'));
    expect(document.activeElement).toBe(screen.getByRole('button', { name: /Customize theme/ }));
    await user.click(screen.getByRole('button', { name: /Customize theme/ }));
    await screen.findByText('Your colours');
    for (const entry of fieldsOf('personalize', true)) {
      if (entry.target.kind !== 'field') continue;
      expect(await reveal(entry.target.find, 200), entry.label).toBe(true);
    }
  });

  it('waits for a screen whose fields arrive later', async () => {
    const found = reveal('Language');
    renderApp(
      <main>
        <Settings />
      </main>,
    );
    expect(await found).toBe(true);
    expect(document.activeElement).toBe(screen.getByRole('combobox', { name: 'Language' }));
  });

  it('gives up on a label that is not there', async () => {
    renderApp(<main />);
    expect(await reveal('No such field', 50)).toBe(false);
  });
});

describe('land', () => {
  it("puts the keyboard on the screen's title", async () => {
    renderApp(
      <main>
        <Settings />
      </main>,
    );
    const title = await screen.findByRole('heading', { name: 'Settings', level: 1 });
    land();
    await waitFor(() => expect(document.activeElement).toBe(title));
  });
});
