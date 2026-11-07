import { expect, it } from 'vitest';
import { remember } from './recent';

it('a new colour goes in front, and the list keeps its cap', () => {
  expect(remember(['#ff0000', '#00ff00'], '#0000ff', 3)).toEqual(['#0000ff', '#ff0000', '#00ff00']);
  expect(remember(['#ff0000', '#00ff00', '#0000ff'], '#ffffff', 3)).toEqual(['#ffffff', '#ff0000', '#00ff00']);
});

it('a colour already in the list leaves the list as it is', () => {
  const list = ['#ff0000', '#00ff00', '#0000ff'];
  expect(remember(list, '#0000ff', 9)).toBe(list);
});
