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
