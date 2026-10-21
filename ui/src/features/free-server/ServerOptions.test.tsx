import { screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MOCK_FREE_LIMITS } from '../../ipc/mockFree';
import { renderWithProviders } from '../../test/render';
import { ServerOptions } from './ServerOptions';
import { initialOptions, settingsOf, useServerOptions } from './serverOptions';

const base = initialOptions(MOCK_FREE_LIMITS);

describe('settingsOf', () => {
  it('is null for an untouched form', () => {
    expect(settingsOf(base)).toBeNull();
  });

  it('starts on the server defaults: survival, easy, the limit, PvP on, whitelist off', () => {
    expect(base).toMatchObject({ gameMode: 'survival', difficulty: 'easy', maxPlayers: '10', pvp: true, whitelist: false, seed: '', motd: '' });
  });

  it('sends only what changed', () => {
    expect(settingsOf({ ...base, pvp: false })).toEqual({ pvp: false });
    expect(settingsOf({ ...base, maxPlayers: '4', whitelist: true })).toEqual({ maxPlayers: 4, whitelist: true });
    expect(settingsOf({ ...base, difficulty: 'hard', seed: ' 12345 ', motd: 'Hola' })).toEqual({ difficulty: 'hard', seed: '12345', motd: 'Hola' });
    // Back to the default is no change.
    expect(settingsOf({ ...base, maxPlayers: '10', seed: '   ' })).toBeNull();
  });

  it('sends hardcore without a difficulty, whatever was picked before', () => {
    expect(settingsOf({ ...base, gameMode: 'hardcore', difficulty: 'normal' })).toEqual({ gameMode: 'hardcore' });
  });
});

function Harness() {
  const options = useServerOptions(MOCK_FREE_LIMITS);
  return (
    <>
      <ServerOptions options={options} />
      <output data-testid="sent">{JSON.stringify(options.settings())}</output>
      <button type="button" onClick={() => options.check()}>
        Check
      </button>
    </>
  );
}

const sentSettings = () => JSON.parse(screen.getByTestId('sent').textContent ?? 'null') as unknown;

describe('ServerOptions', () => {
  it('is closed by default and says what the server gets', () => {
    renderWithProviders(<Harness />);
    const toggle = screen.getByRole('button', { name: /^Options/ });
    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    expect(toggle).toHaveTextContent('Survival, Easy, up to 10 players');
    expect(screen.queryByRole('radio', { name: 'Survival' })).toBeNull();
  });

  it('opens on the defaults', async () => {
    const { user } = renderWithProviders(<Harness />);
    await user.click(screen.getByRole('button', { name: /^Options/ }));
    expect(screen.getByRole('button', { name: /^Options/ })).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByRole('radio', { name: 'Survival' })).toBeChecked();
    expect(screen.getByRole('combobox', { name: 'Difficulty' })).toHaveValue('easy');
    expect(screen.getByRole('spinbutton', { name: 'Max players' })).toHaveValue(10);
    expect(screen.getByRole('switch', { name: 'PvP' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('switch', { name: 'Whitelist' })).toHaveAttribute('aria-checked', 'false');
    expect(screen.getByText('Only players you add in Manage → Players can join.')).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'World seed' })).toHaveValue('');
    expect(screen.getByRole('textbox', { name: 'Server list message' })).toHaveValue('');
    expect(sentSettings()).toBeNull();
  });

  it('locks the difficulty to Hard with Hardcore, and gives back the earlier pick after', async () => {
    const { user } = renderWithProviders(<Harness />);
    await user.click(screen.getByRole('button', { name: /^Options/ }));
    const difficulty = screen.getByRole('combobox', { name: 'Difficulty' });
    await user.selectOptions(difficulty, 'normal');
    await user.click(screen.getByRole('radio', { name: 'Hardcore' }));
    expect(difficulty).toHaveValue('hard');
    expect(difficulty).toBeDisabled();
    expect(screen.getByText('Hardcore is always on Hard.')).toBeInTheDocument();
    expect(sentSettings()).toEqual({ gameMode: 'hardcore' });
    await user.click(screen.getByRole('radio', { name: 'Survival' }));
    expect(difficulty).toBeEnabled();
    expect(difficulty).toHaveValue('normal');
    expect(sentSettings()).toEqual({ difficulty: 'normal' });
  });

  it('bounds max players from 1 to the limit', async () => {
    const { user } = renderWithProviders(<Harness />);
    await user.click(screen.getByRole('button', { name: /^Options/ }));
    const max = screen.getByRole('spinbutton', { name: 'Max players' });
    expect(max).toHaveAttribute('min', '1');
    expect(max).toHaveAttribute('max', '10');
    for (const bad of ['0', '11', '2.5', '']) {
      await user.clear(max);
      if (bad) await user.type(max, bad);
      await user.click(screen.getByRole('button', { name: 'Check' }));
      expect(max).toHaveAttribute('aria-invalid', 'true');
      expect(screen.getByText('Pick a number from 1 to 10.')).toBeInTheDocument();
    }
    await user.clear(max);
    await user.type(max, '3');
    expect(max).not.toHaveAttribute('aria-invalid');
    expect(sentSettings()).toEqual({ maxPlayers: 3 });
  });

  it('opens itself to show a problem with an option', async () => {
    const { user } = renderWithProviders(<Harness />);
    await user.click(screen.getByRole('button', { name: /^Options/ }));
    await user.type(screen.getByRole('textbox', { name: 'Server list message' }), '§cRed');
    await user.click(screen.getByRole('button', { name: /^Options/ }));
    await user.click(screen.getByRole('button', { name: 'Check' }));
    const motd = screen.getByRole('textbox', { name: 'Server list message' });
    expect(motd).toHaveAttribute('aria-invalid', 'true');
    expect(within(motd.parentElement!).getByText('Use a shorter message, without §.')).toBeInTheDocument();
  });
});
