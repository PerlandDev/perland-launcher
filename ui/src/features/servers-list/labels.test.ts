import { afterEach, describe, expect, it } from 'vitest';
import i18n from '../../i18n';
import { gameLine } from './labels';

const t = i18n.t.bind(i18n);
afterEach(() => i18n.changeLanguage('en'));

const game = (version: string | null, loader: string | null = null) => ({ pack: null, game: { version, loader, loaderVersion: null } });

describe('gameLine', () => {
  it('says_latest_in_words', () => {
    // A server on itzg's `LATEST` (any case) runs the newest release: the token is not a version to show.
    expect(gameLine(t, game('LATEST'))).toBe('Vanilla (latest)');
    expect(gameLine(t, game('latest', 'fabric'))).toBe('Fabric (latest)');
    expect(gameLine(t, game('1.21.4'))).toBe('Vanilla 1.21.4');
    i18n.changeLanguage('es');
    expect(gameLine(t, game('LATEST'))).toBe('Vanilla (última versión)');
  });
});
