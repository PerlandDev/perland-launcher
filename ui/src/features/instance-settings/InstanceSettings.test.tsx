import { fireEvent, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { renderApp, sent } from '../../test/backend';
import { InstanceSettings } from './InstanceSettings';

it('saves memory and a tested custom Java', async () => {
  const { user, calls } = renderApp(<InstanceSettings id="asalith-fields" />);
  const slider = await screen.findByRole('slider', { name: 'Memory' });
  fireEvent.change(slider, { target: { value: '10240' } });
  expect(screen.getByText('10 GB')).toBeInTheDocument();
  expect(screen.getByText(/more than half of this computer's memory/)).toBeInTheDocument();
  await user.click(screen.getByRole('radio', { name: 'A Java installation I choose' }));
  await user.click(screen.getByRole('button', { name: 'Choose…' }));
  await user.click(screen.getByRole('button', { name: 'Test' }));
  expect(await screen.findByText('Java 17 works')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { java: { memoryMb: number; path: string } } };
  expect(saved.instance.java).toEqual({ memoryMb: 10240, path: 'C:/Users/player/Downloads/lithium-fabric-0.11.2.jar' });
  expect(await screen.findByText('Saved')).toBeInTheDocument();
});

it('keeps the current loader and its version, even when the list no longer offers it', async () => {
  renderApp(<InstanceSettings id="asalith-fields" />);
  expect(await screen.findByRole('radio', { name: 'Fabric' })).toBeChecked();
  // The fixture list offers 0.19.x; the instance uses 0.16.14 and must keep it.
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('0.16.14');
});

it('saves a Forge instance with its game unchanged and explains that the version cannot change yet', async () => {
  const forge = { minecraft: '1.20.1', loader: { kind: 'forge', version: '47.2.0' } } as const;
  const { user, calls, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  backend.state.listing.instances[0]!.game = structuredClone(forge);
  expect(await screen.findByText(/This instance uses Forge\. Changing its version isn't available in the launcher yet\./)).toBeInTheDocument();
  expect(screen.queryByRole('combobox', { name: 'Minecraft version' })).not.toBeInTheDocument();
  expect(screen.queryByRole('radio', { name: 'Fabric' })).not.toBeInTheDocument();
  await user.click(await screen.findByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { game: unknown } };
  expect(saved.instance.game).toEqual(forge);
});
