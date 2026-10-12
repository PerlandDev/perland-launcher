import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { renderWithProviders } from '../../test/render';
import { StatusPill } from './StatusPill';

describe('StatusPill', () => {
  it.each([
    ['online', { online: 3, max: 20 }, false, 'Online · 3/20'],
    ['online', { online: 9, max: null }, false, 'Online · 9'],
    ['online', null, false, 'Online'],
    ['starting', null, false, 'Starting'],
    ['sleeping', null, false, 'Sleeping · join to wake it'],
    ['offline', null, false, 'Offline'],
    [null, null, false, 'Unknown'],
    ['online', { online: 3, max: 20 }, true, 'Unknown'],
  ] as const)('%s %j gone=%s reads %s', (status, players, gone, word) => {
    renderWithProviders(<StatusPill status={status} players={players} gone={gone} />);
    expect(screen.getByRole('status')).toHaveTextContent(word);
    expect(screen.getByRole('status')).toHaveAccessibleName(`Server status: ${word}`);
  });
});
