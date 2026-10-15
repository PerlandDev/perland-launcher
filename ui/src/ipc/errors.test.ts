import { afterEach, describe, expect, it } from 'vitest';
import i18n from '../i18n';
import { describeError, isLauncherError, supportText, taskLabel } from './errors';

const notFound = {
  category: 'loader',
  code: 'minecraft_version_not_found',
  message: "Minecraft 9.9 doesn't exist",
  fix: 'Check the version number',
  detail: 'manifest has no 9.9',
  params: { version: '9.9' },
} as const;

afterEach(() => i18n.changeLanguage('en'));

describe('describeError', () => {
  it('translates by code with params', async () => {
    await i18n.changeLanguage('es');
    const text = describeError(i18n, notFound);
    expect(text.message).toBe('Minecraft 9.9 no existe.');
    expect(text.fix).toBe('Elige una versión de la lista.');
    expect(text.detail).toBe('manifest has no 9.9');
  });

  it("falls back to the core's English for codes it doesn't know", () => {
    const text = describeError(i18n, { ...notFound, code: 'brand_new_code' });
    expect(text.message).toBe("Minecraft 9.9 doesn't exist");
    expect(text.fix).toBe('Check the version number');
  });

  it('names the damaged .mrpack entry, and drops the name when the whole archive is damaged', async () => {
    await i18n.changeLanguage('es');
    const corrupt = { category: 'pack', code: 'mrpack_corrupt', message: 'x', fix: null, detail: null } as const;
    expect(describeError(i18n, { ...corrupt, params: { file: 'config/a.txt' } }).message).toBe(
      'Este archivo .mrpack está dañado (config/a.txt).',
    );
    expect(describeError(i18n, { ...corrupt, params: { context: 'archive' } }).message).toBe(
      'Este archivo .mrpack está dañado.',
    );
    await i18n.changeLanguage('en');
    expect(describeError(i18n, { ...corrupt, params: { context: 'archive' } }).message).toBe(
      'This .mrpack file is damaged.',
    );
  });

  it('words what a pack needs from a context and raw values, never English params', async () => {
    const unsupported = { category: 'pack', code: 'pack_unsupported', message: 'x', fix: null, detail: null } as const;
    const loader = { ...unsupported, params: { context: 'loader', name: 'liteloader' } };
    const game = { ...unsupported, params: { context: 'mrpack_game', game: 'terraria' } };
    expect(describeError(i18n, loader).message).toBe("This pack needs the liteloader loader, which the launcher can't run yet.");
    expect(describeError(i18n, game).message).toBe('This .mrpack is for terraria, not Minecraft.');
    expect(describeError(i18n, { ...unsupported, params: {} }).message).toBe(
      "This pack uses something the launcher can't handle yet.",
    );
    await i18n.changeLanguage('es');
    expect(describeError(i18n, loader).message).toBe('Este pack necesita el loader liteloader, que el launcher todavía no puede usar.');
    expect(describeError(i18n, game).message).toBe('Este .mrpack es para terraria, no para Minecraft.');
  });

  it('says what is wrong with a pack when no file is to blame', async () => {
    const invalid = { category: 'pack', code: 'pack_invalid', message: 'x', fix: null, detail: null } as const;
    const noChecksum = { ...invalid, params: { context: 'no_checksum' } };
    const modrinth = { ...invalid, params: { context: 'modrinth_data' } };
    expect(describeError(i18n, noChecksum).message).toBe('A pack file has no checksum the launcher can use.');
    expect(describeError(i18n, modrinth)).toMatchObject({
      message: "Modrinth sent pack details the launcher couldn't read.",
      fix: 'Try again in a few minutes.',
    });
    await i18n.changeLanguage('es');
    expect(describeError(i18n, noChecksum).message).toBe('Un archivo del pack no tiene un checksum que el launcher pueda usar.');
    expect(describeError(i18n, { ...invalid, params: { file: 'index.toml' } }).message).toBe(
      'El archivo index.toml del pack está dañado o no es un pack.',
    );
  });

  it('words a missing loader version and an unknown host without English params', async () => {
    const latest = {
      category: 'loader',
      code: 'loader_version_not_found',
      message: 'x',
      fix: null,
      detail: null,
      params: { context: 'latest', loader: 'fabric', version: '1.21.1' },
    } as const;
    const trouble = {
      category: 'network',
      code: 'host_trouble',
      message: 'x',
      fix: null,
      detail: null,
      params: { context: 'unknown_host' },
    } as const;
    expect(describeError(i18n, latest).message).toBe('fabric has no version for Minecraft 1.21.1 yet.');
    expect(describeError(i18n, trouble).message).toBe('The download server is having trouble right now.');
    await i18n.changeLanguage('es');
    expect(describeError(i18n, latest).message).toBe('fabric todavía no tiene una versión para Minecraft 1.21.1.');
    expect(describeError(i18n, trouble).message).toBe('El servidor de descargas tiene problemas en este momento.');
  });

  it('wraps anything that is not a LauncherError', () => {
    expect(isLauncherError(new Error('x'))).toBe(false);
    const text = describeError(i18n, new Error('boom'));
    expect(text.message).toBe('Something unexpected went wrong.');
    expect(text.detail).toBe('boom');
  });
});

it('labels progress tasks with their params', async () => {
  expect(taskLabel(i18n, { key: 'install_minecraft', params: { version: '1.20.1' } })).toBe(
    'Installing Minecraft 1.20.1',
  );
  expect(taskLabel(i18n, { key: 'mystery', params: {} })).toBe('mystery');
});

it('builds support text from lines', () => {
  expect(supportText(['a', 'b'])).toBe('a\nb');
});
