/** Shipped builds are an app, not a browser page: no page menu (Save as, Print, Inspect) and no browser shortcuts (reload, print, save, find, view source). Text fields keep their menu for copy and paste. */
const CTRL_KEYS = new Set(['r', 'p', 's', 'u', 'f', 'g', 'j', 'h']);
// Only these get the webview's edit menu; sliders, checkboxes and dropdowns get the page menu.
const TEXT_INPUTS = new Set(['text', 'search', 'email', 'url', 'tel', 'password', 'number']);

function takesText(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLTextAreaElement || target.closest('[contenteditable=""], [contenteditable="true"]')) return true;
  return target instanceof HTMLInputElement && TEXT_INPUTS.has(target.type);
}

function blockedKey(e: KeyboardEvent): boolean {
  if (e.key === 'F3' || e.key === 'F5' || e.key === 'F7') return true;
  return (e.ctrlKey || e.metaKey) && !e.altKey && CTRL_KEYS.has(e.key.toLowerCase());
}

/** Returns a function that removes the guard. */
export function installBrowserGuard(target: Document): () => void {
  const onMenu = (e: MouseEvent) => {
    if (!takesText(e.target)) e.preventDefault();
  };
  const onKey = (e: KeyboardEvent) => {
    if (blockedKey(e)) e.preventDefault();
  };
  target.addEventListener('contextmenu', onMenu);
  target.addEventListener('keydown', onKey, true);
  return () => {
    target.removeEventListener('contextmenu', onMenu);
    target.removeEventListener('keydown', onKey, true);
  };
}
