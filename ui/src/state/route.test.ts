import { afterEach, expect, it } from 'vitest';
import { instanceOf, lastInstance, navigate, rememberInstance, resetRoute, routeSnapshot } from './route';

afterEach(resetRoute);

it('navigates and reports the instance a route is about', () => {
  expect(routeSnapshot()).toEqual({ name: 'home', id: null });
  navigate({ name: 'mods', id: 'pack' });
  expect(instanceOf(routeSnapshot())).toBe('pack');
  navigate({ name: 'settings' });
  expect(instanceOf(routeSnapshot())).toBeNull();
});

it('remembers the last instance when storage works', () => {
  rememberInstance('pack');
  expect(lastInstance()).toBe('pack');
});
