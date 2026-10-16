import { act, fireEvent, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import type { Instance } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
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
  expect(saved.instance.java).toEqual({ memoryMb: 10240, path: 'C:/Users/player/Downloads/lithium-fabric-0.11.2.jar', args: '' });
  expect(await screen.findByText('Saved')).toBeInTheDocument();
});

it('keeps the current loader and its version, even when the list no longer offers it', async () => {
  renderApp(<InstanceSettings id="asalith-fields" />);
  expect(await screen.findByRole('radio', { name: 'Fabric' })).toBeChecked();
  // The fixture list offers 0.19.x; the instance uses 0.16.14 and must keep it.
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('0.16.14');
});

it('shows a Forge instance on Forge and saves its game unchanged', async () => {
  const forge = { minecraft: '1.20.1', loader: { kind: 'forge', version: '47.2.0' } } as const;
  const { user, calls, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  backend.state.listing.instances[0]!.game = structuredClone(forge);
  expect(await screen.findByRole('radio', { name: 'Forge' })).toBeChecked();
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('47.2.0');
  await user.click(await screen.findByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { game: unknown } };
  expect(saved.instance.game).toEqual(forge);
});

const failure = { category: 'game', code: 'java_test_failed', message: 'nope', fix: null, detail: null, params: {} };

async function chooseCustomJava(user: ReturnType<typeof renderApp>['user']) {
  await screen.findByRole('slider', { name: 'Memory' });
  await user.click(screen.getByRole('radio', { name: 'A Java installation I choose' }));
  await user.click(screen.getByRole('button', { name: 'Choose…' }));
}

it('shows why a Java test failed', async () => {
  const { user, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'java_test' ? Promise.reject(failure) : handle(cmd, args)));
  await chooseCustomJava(user);
  await user.click(screen.getByRole('button', { name: 'Test' }));
  expect(await screen.findByRole('alert')).toHaveTextContent("That doesn't look like a working Java.");
  expect(screen.queryByText('Java 17 works')).not.toBeInTheDocument();
});

it('sends one Java test for a double click while the first is running', async () => {
  const { user, calls, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  const handle = backend.handle;
  let finish = () => {};
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
    cmd === 'java_test' ? new Promise((resolve) => (finish = () => resolve({ major: 17 }))) : handle(cmd, args),
  );
  await chooseCustomJava(user);
  await user.dblClick(screen.getByRole('button', { name: 'Test' }));
  expect(sent(calls, 'java_test')).toHaveLength(1);
  await act(async () => finish());
  expect(await screen.findByText('Java 17 works')).toBeInTheDocument();
});

it('never shows a result next to a different, untested Java path', async () => {
  const { user, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  const handle = backend.handle;
  let finish = () => {};
  let picks = 0;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => {
    if (cmd === 'java_test') return new Promise((resolve) => (finish = () => resolve({ major: 17 })));
    if (cmd === 'plugin:dialog|open' && ++picks === 2) return Promise.resolve('C:/other/java.exe');
    return handle(cmd, args);
  });
  await chooseCustomJava(user);
  await user.click(screen.getByRole('button', { name: 'Test' }));
  expect(screen.getByRole('button', { name: 'Testing…' })).toBeDisabled();
  await user.click(screen.getByRole('button', { name: 'Choose…' }));
  expect(await screen.findByText('C:/other/java.exe')).toBeInTheDocument();
  await act(async () => finish());
  expect(screen.queryByText('Java 17 works')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Test' })).toBeEnabled();
});

it('shows a failure to open the file dialog instead of leaving it unhandled', async () => {
  const { user, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'plugin:dialog|open' ? Promise.reject(new Error('no dialog')) : handle(cmd, args)));
  await chooseCustomJava(user);
  expect(await screen.findByRole('alert')).toBeInTheDocument();
});

it('announces the Java result and the save as status messages', async () => {
  const { user } = renderApp(<InstanceSettings id="asalith-fields" />);
  await chooseCustomJava(user);
  await user.click(screen.getByRole('button', { name: 'Test' }));
  expect(await screen.findByText('Java 17 works')).toHaveAttribute('role', 'status');
  await user.click(screen.getByRole('button', { name: 'Save' }));
  expect(await screen.findByText('Saved')).toHaveAttribute('role', 'status');
});

it('clears "Saved" and a failed save as soon as the form changes again', async () => {
  const { user, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  await user.click(await screen.findByRole('button', { name: 'Save' }));
  expect(await screen.findByText('Saved')).toBeInTheDocument();
  await user.type(screen.getByRole('textbox', { name: 'Name' }), '!');
  expect(screen.queryByText('Saved')).not.toBeInTheDocument();

  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'instance_update' ? Promise.reject(failure) : handle(cmd, args)));
  await user.click(screen.getByRole('button', { name: 'Save' }));
  expect(await screen.findByRole('alert')).toBeInTheDocument();
  fireEvent.change(screen.getByRole('slider', { name: 'Memory' }), { target: { value: '2048' } });
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});

it('will not save custom Java mode without a chosen file', async () => {
  const { user } = renderApp(<InstanceSettings id="asalith-fields" />);
  await screen.findByRole('slider', { name: 'Memory' });
  await user.click(screen.getByRole('radio', { name: 'A Java installation I choose' }));
  expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
});

it('saves an untouched instance with its game exactly as it was, loader version included', async () => {
  const { user, calls } = renderApp(<InstanceSettings id="asalith-fields" />);
  await screen.findByRole('option', { name: /0\.19\.5/ });
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { game: unknown } };
  expect(saved.instance.game).toEqual({ minecraft: '1.20.1', loader: { kind: 'fabric', version: '0.16.14' } });
});

it('saves an instance whose Minecraft version the catalog does not list unchanged', async () => {
  const { user, calls, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  backend.state.listing.instances[0]!.game = { minecraft: '1.2.5', loader: null };
  const select = await screen.findByRole('combobox', { name: 'Minecraft version' });
  await screen.findByRole('option', { name: '26.3' });
  expect(select).toHaveValue('1.2.5');
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { game: unknown } };
  expect(saved.instance.game).toEqual({ minecraft: '1.2.5', loader: null });
});

it("sets a pack's update policy and leaves its game to the pack", async () => {
  const { user, calls } = renderApp(<InstanceSettings id="sky-islands" />);
  expect(await screen.findByText('The pack picks the Minecraft version and loader.')).toBeInTheDocument();
  expect(screen.queryByRole('combobox', { name: 'Minecraft version' })).not.toBeInTheDocument();
  expect(screen.getByRole('radio', { name: 'Ask me' })).toBeChecked();
  await user.click(screen.getByRole('radio', { name: 'Automatic' }));
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const [sentArgs] = sent(calls, 'instance_update') as { instance: Instance }[];
  expect(sentArgs!.instance.updatePolicy).toBe('auto');
  expect(sentArgs!.instance.game).toEqual({ minecraft: '1.20.1', loader: { kind: 'forge', version: '47.4.22' } });
});

it('shows a server section and no Updates control for a server instance', async () => {
  const { user } = renderApp(<InstanceSettings id="survival" />, { server: 'online' });
  expect(await screen.findByText('Invite from play.example.com')).toBeInTheDocument();
  expect(screen.queryByRole('radio', { name: 'Automatic' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Copy link' }));
  expect(await navigator.clipboard.readText()).toBe(`https://play.example.com/${'A'.repeat(43)}`);
  await user.click(screen.getByRole('button', { name: 'Use a new link' }));
  expect(routeSnapshot()).toEqual({ name: 'joinServer', invite: '', relinkFor: 'survival' });
});

it('keeps a running server on its link, and says why', async () => {
  renderApp(<InstanceSettings id="survival" />, { server: 'online' });
  await screen.findByText('Invite from play.example.com');
  act(() => runStore.apply({ type: 'run_state', instance: 'survival', state: 'syncing', exit: null, update: null }));
  const button = screen.getByRole('button', { name: 'Use a new link' });
  expect(button).toBeDisabled();
  expect(button).toHaveAccessibleDescription('Stop the game to use a new link.');
  act(() => runStore.apply({ type: 'run_state', instance: 'survival', state: 'idle', exit: null, update: null }));
  expect(screen.getByRole('button', { name: 'Use a new link' })).toBeEnabled();
  expect(screen.queryByText('Stop the game to use a new link.')).not.toBeInTheDocument();
});

it('keeps the Server section off plain and pack instances', async () => {
  renderApp(<InstanceSettings id="sky-islands" />);
  await screen.findByRole('radio', { name: 'Automatic' });
  expect(screen.queryByRole('heading', { name: 'Server' })).not.toBeInTheDocument();
});

it('saves the Java arguments as typed', async () => {
  const { user, calls } = renderApp(<InstanceSettings id="asalith-fields" />);
  const field = await screen.findByRole('textbox', { name: 'Java arguments' });
  expect(field).toHaveValue('');
  await user.click(field);
  await user.paste('-XX:+UseG1GC -Dx="a b"');
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { java: unknown } };
  expect(saved.instance.java).toEqual({ memoryMb: 4096, path: null, args: '-XX:+UseG1GC -Dx="a b"' });
  expect(await screen.findByText('Saved')).toBeInTheDocument();
});

it('shows a stored Java arguments value and saves it unchanged', async () => {
  const { user, calls, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  backend.state.listing.instances[0]!.java.args = '-XX:+UseZGC -Dname="two words"';
  const field = await screen.findByRole('textbox', { name: 'Java arguments' });
  expect(field).toHaveValue('-XX:+UseZGC -Dname="two words"');
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { java: { args: string } } };
  expect(saved.instance.java.args).toBe('-XX:+UseZGC -Dname="two words"');
});

it('shows a refused argument under the field', async () => {
  const { user } = renderApp(<InstanceSettings id="asalith-fields" />);
  const field = await screen.findByRole('textbox', { name: 'Java arguments' });
  await user.click(field);
  await user.paste('-Xmx4G');
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const message = 'Memory is set with the slider, not in the Java arguments.';
  expect(await screen.findByText(message)).toBeInTheDocument();
  expect(field).toHaveAttribute('aria-invalid', 'true');
  expect(field).toHaveAccessibleDescription(message);
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  // Focus moves to the field, so the message isn't left off-screen behind the sticky Save bar.
  expect(field).toHaveFocus();
  await user.clear(field);
  expect(screen.queryByText(message)).not.toBeInTheDocument();
  expect(field).not.toHaveAttribute('aria-invalid');
});
