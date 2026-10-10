import { screen, waitFor } from '@testing-library/react';
import { expect, it } from 'vitest';
import type { GameSpec } from '../../ipc/schema';
import { renderApp } from '../../test/backend';
import { useVersionChoice } from './useVersionChoice';

function Probe({ initial }: { initial: GameSpec }) {
  const choice = useVersionChoice(initial);
  return (
    <p data-testid="probe">
      {JSON.stringify({ minecraft: choice.mc, loader: choice.loader, chosenLoader: choice.chosenLoader, resolved: choice.resolved.loader })}
    </p>
  );
}

it('keeps a Forge instance on Forge with its version', async () => {
  renderApp(<Probe initial={{ minecraft: '1.20.1', loader: { kind: 'forge', version: '47.2.0' } }} />);
  await waitFor(() =>
    expect(JSON.parse(screen.getByTestId('probe').textContent ?? '')).toEqual({
      minecraft: '1.20.1',
      loader: 'forge',
      chosenLoader: '47.2.0',
      resolved: { kind: 'forge', version: '47.2.0' },
    }),
  );
});

function LoadedProbe({ initial }: { initial: GameSpec }) {
  const choice = useVersionChoice(initial);
  return <p data-testid="loaded">{JSON.stringify({ loaded: choice.versions.isSuccess, minecraft: choice.mc, resolved: choice.resolved.minecraft })}</p>;
}

it('keeps an instance Minecraft version the catalog does not list', async () => {
  renderApp(<LoadedProbe initial={{ minecraft: '1.2.5', loader: null }} />);
  await waitFor(() => expect(JSON.parse(screen.getByTestId('loaded').textContent ?? '').loaded).toBe(true));
  expect(JSON.parse(screen.getByTestId('loaded').textContent ?? '')).toEqual({ loaded: true, minecraft: '1.2.5', resolved: '1.2.5' });
});
