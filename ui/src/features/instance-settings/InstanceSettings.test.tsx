import { act, fireEvent, screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import type { Instance } from '../../ipc/schema';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { InstanceSettings } from './InstanceSettings';

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
}

const TEMURIN_17 = 'C:/Program Files/Eclipse Adoptium/jdk-17.0.12.7-hotspot/bin/javaw.exe';
const TEMURIN_21 = 'C:/Program Files/Eclipse Adoptium/jdk-21.0.4.7-hotspot/bin/javaw.exe';

it('says which Java Automatic uses for this Minecraft version', async () => {
  renderApp(<InstanceSettings id="asalith-fields" />);
  expect(await screen.findByRole('radio', { name: 'Automatic (recommended)' })).toBeChecked();
  expect(await screen.findByText("Java 17 · Automatic (Mojang's official Java, managed by Perland)")).toBeInTheDocument();
});

it('saves memory and a Java found on this computer', async () => {
  const { user, calls } = renderApp(<InstanceSettings id="asalith-fields" />);
  const slider = await screen.findByRole('slider', { name: 'Memory' });
  fireEvent.change(slider, { target: { value: '10240' } });
  expect(screen.getByText('10 GB')).toBeInTheDocument();
  expect(screen.getByText(/more than half of this computer's memory/)).toBeInTheDocument();
  await chooseCustomJava(user);
  const found = await screen.findByRole('radiogroup', { name: 'Java on this computer' });
  // The one that fits the instance's Minecraft (1.20.1 → Java 17) comes first.
  const options = within(found).getAllByRole('radio');
  expect(options[0]).toHaveAccessibleName(/Eclipse Temurin 17\.0\.12/);
  expect(within(options[0]!.closest('label')!).getByText(/Right for this version/)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  await user.click(options[0]!);
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { java: unknown } };
  expect(saved.instance.java).toEqual({ memoryMb: 10240, path: TEMURIN_17, args: '' });
  expect(await screen.findByText('Saved')).toHaveAttribute('role', 'status');
});

it('warns when the chosen Java does not match, and saves only after "use it anyway"', async () => {
  const { user, calls } = renderApp(<InstanceSettings id="asalith-fields" />);
  await chooseCustomJava(user);
  await user.click(await screen.findByRole('radio', { name: /Oracle 1\.8\.0_421/ }));
  expect(screen.getByRole('alert')).toHaveTextContent("Minecraft 1.20.1 needs Java 17. Java 8 is too old, so the game won't start.");
  const save = screen.getByRole('button', { name: 'Save' });
  expect(save).toBeDisabled();
  await user.click(screen.getByRole('radio', { name: /Eclipse Temurin 21\.0\.4/ }));
  expect(screen.getByRole('alert')).toHaveTextContent('Minecraft 1.20.1 is made for Java 17. Java 21 is newer and may not work, especially with mods.');
  await user.click(screen.getByRole('switch', { name: 'Use this Java anyway' }));
  expect(save).toBeEnabled();
  await user.click(save);
  const saved = sent(calls, 'instance_update')[0] as { instance: { java: { path: string } } };
  expect(saved.instance.java.path).toBe(TEMURIN_21);
});

it('checks a Java the player browses to and judges it against the version', async () => {
  const { user, calls, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'plugin:dialog|open' ? Promise.resolve('D:/jdks/jdk-21/bin/javaw.exe') : handle(cmd, args)));
  await chooseCustomJava(user);
  await user.click(screen.getByRole('button', { name: 'Browse…' }));
  expect(await screen.findByRole('radio', { name: /Java 21\.0\.4.*D:\/jdks/ })).toBeChecked();
  expect(sent(calls, 'java_test')[0]).toEqual({ path: 'D:/jdks/jdk-21/bin/javaw.exe', minecraft: '1.20.1' });
  expect(await screen.findByRole('alert')).toHaveTextContent('Java 21 is newer');
});

it('shows why a browsed Java failed its check and will not save it', async () => {
  const { user, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'java_test' ? Promise.reject(failure) : handle(cmd, args)));
  await chooseCustomJava(user);
  await user.click(screen.getByRole('button', { name: 'Browse…' }));
  expect(await screen.findByRole('alert')).toHaveTextContent("That doesn't look like a working Java.");
  expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
});

it('keeps a stored Java chosen and saves it unchanged without asking again', async () => {
  const { user, calls, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  backend.state.listing.instances[0]!.java.path = TEMURIN_21;
  expect(await screen.findByRole('radio', { name: /Eclipse Temurin 21\.0\.4/ })).toBeChecked();
  expect(screen.getByRole('switch', { name: 'Use this Java anyway' })).toBeChecked();
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { java: { path: string } } };
  expect(saved.instance.java.path).toBe(TEMURIN_21);
});

it('says so when no Java is found on this computer', async () => {
  const { user, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) =>
    cmd === 'java_overview' ? Promise.resolve({ required: 17, installs: [] }) : handle(cmd, args),
  );
  await chooseCustomJava(user);
  expect(await screen.findByText("We didn't find Java on this computer. Use Browse… to pick one.")).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
});

it('shows a failure to open the file dialog instead of leaving it unhandled', async () => {
  const { user, backend } = renderApp(<InstanceSettings id="asalith-fields" />);
  const handle = backend.handle;
  vi.spyOn(backend, 'handle').mockImplementation((cmd, args) => (cmd === 'plugin:dialog|open' ? Promise.reject(new Error('no dialog')) : handle(cmd, args)));
  await chooseCustomJava(user);
  await user.click(screen.getByRole('button', { name: 'Browse…' }));
  expect(await screen.findByRole('alert')).toBeInTheDocument();
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

it('keeps the Server section off pack instances', async () => {
  renderApp(<InstanceSettings id="sky-islands" />);
  await screen.findByRole('radio', { name: 'Automatic' });
  expect(screen.queryByRole('heading', { name: 'Server' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Link to a server' })).not.toBeInTheDocument();
});

it('offers a plain instance a link to a server, except while it runs', async () => {
  const { user } = renderApp(<InstanceSettings id="asalith-fields" />);
  expect(await screen.findByRole('heading', { name: 'Server' })).toBeInTheDocument();
  expect(screen.getByText('Link this instance to a Perland server and it updates with the server.')).toBeInTheDocument();
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null, update: null }));
  const button = screen.getByRole('button', { name: 'Link to a server' });
  expect(button).toBeDisabled();
  expect(button).toHaveAccessibleDescription('Stop the game to link this instance.');
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'idle', exit: null, update: null }));
  await user.click(screen.getByRole('button', { name: 'Link to a server' }));
  expect(routeSnapshot()).toEqual({ name: 'joinServer', invite: '', linkFor: 'asalith-fields' });
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
