/** Shipped builds are an app, not a browser page: no page menu (Save as, Print, Inspect) and no browser shortcuts (reload, print, save, find, view source). Text fields keep their menu for copy and paste. */
const EDITABLE = 'input, textarea, select, [contenteditable=""], [contenteditable="true"]';
const CTRL_KEYS = new Set(['r', 'p', 's', 'u', 'f', 'g', 'j', 'h']);

function blockedKey(e: KeyboardEvent): boolean {
  if (e.key === 'F5' || e.key === 'F7') return true;
  return (e.ctrlKey || e.metaKey) && !e.altKey && CTRL_KEYS.has(e.key.toLowerCase());
}

/** Returns a function that removes the guard. */
export function installBrowserGuard(target: Document): () => void {
  const onMenu = (e: MouseEvent) => {
    if (!(e.target instanceof Element && e.target.closest(EDITABLE))) e.preventDefault();
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
