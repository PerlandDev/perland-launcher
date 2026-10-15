import { afterEach, expect, it } from 'vitest';
import { installBrowserGuard } from './browserGuard';

let uninstall = () => {};
afterEach(() => {
  uninstall();
  uninstall = () => {};
  document.body.innerHTML = '';
});

function fire(target: Element, event: Event) {
  target.dispatchEvent(event);
  return event.defaultPrevented;
}
const menu = () => new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
const key = (init: KeyboardEventInit) => new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });

it('blocks the page context menu but keeps it in text fields', () => {
  uninstall = installBrowserGuard(document);
  document.body.innerHTML =
    '<div><span id="s">x</span></div><input id="i"><textarea id="t"></textarea><div contenteditable="true"><b id="c">x</b></div>';
  expect(fire(document.getElementById('s')!, menu())).toBe(true);
  expect(fire(document.getElementById('i')!, menu())).toBe(false);
  expect(fire(document.getElementById('t')!, menu())).toBe(false);
  expect(fire(document.getElementById('c')!, menu())).toBe(false);
});

it('keeps the menu only where text is typed: not on sliders, checkboxes or dropdowns', () => {
  uninstall = installBrowserGuard(document);
  document.body.innerHTML =
    '<input id="search" type="search"><input id="num" type="number"><input id="range" type="range"><input id="box" type="checkbox"><select id="sel"><option>a</option></select>';
  expect(fire(document.getElementById('search')!, menu())).toBe(false);
  expect(fire(document.getElementById('num')!, menu())).toBe(false);
  // Chromium shows the page menu (Save as, Print) on these, not an edit menu.
  expect(fire(document.getElementById('range')!, menu())).toBe(true);
  expect(fire(document.getElementById('box')!, menu())).toBe(true);
  expect(fire(document.getElementById('sel')!, menu())).toBe(true);
});

it('blocks reload, print, save, find and view-source, not editing keys', () => {
  uninstall = installBrowserGuard(document);
  const body = document.body;
  const blocked: KeyboardEventInit[] = [
    { key: 'F3' },
    { key: 'F5' },
    { key: 'F7' },
    { key: 'r', ctrlKey: true },
    { key: 'p', ctrlKey: true },
    { key: 's', ctrlKey: true },
    { key: 'u', ctrlKey: true },
    { key: 'f', ctrlKey: true },
    { key: 'R', ctrlKey: true, shiftKey: true },
    { key: 'p', metaKey: true },
  ];
  for (const k of blocked) expect(fire(body, key(k)), JSON.stringify(k)).toBe(true);
  const allowed: KeyboardEventInit[] = [
    { key: 'a' },
    { key: 'r' },
    { key: 'c', ctrlKey: true },
    { key: 'v', ctrlKey: true },
    { key: 'x', ctrlKey: true },
    { key: 'a', ctrlKey: true },
    { key: 'z', ctrlKey: true },
    { key: 'Tab' },
    { key: 'ArrowDown' },
  ];
  for (const k of allowed) expect(fire(body, key(k)), JSON.stringify(k)).toBe(false);
});

it('uninstalls cleanly', () => {
  installBrowserGuard(document)();
  document.body.innerHTML = '<div id="d"></div>';
  expect(fire(document.getElementById('d')!, menu())).toBe(false);
  expect(fire(document.body, key({ key: 'F5' }))).toBe(false);
});
