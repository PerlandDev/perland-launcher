import { screen } from '@testing-library/react';
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

it('drops the version of a loader the picker cannot offer instead of carrying it over', async () => {
  renderApp(<Probe initial={{ minecraft: '1.20.1', loader: { kind: 'forge', version: '47.2.0' } }} />);
  expect(JSON.parse((await screen.findByTestId('probe')).textContent ?? '')).toEqual({
    minecraft: '1.20.1',
    loader: 'none',
    chosenLoader: null,
    resolved: null,
  });
});
