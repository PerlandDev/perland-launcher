// How the rest of the shell opens Go to (the title bar's key), and the shortcut it answers to on this computer.

const listeners = new Set<() => void>();

/** Open Go to, if the shell has it mounted and no other dialog is open. */
export function openQuickJump() {
  listeners.forEach((listener) => listener());
}

export function onOpenQuickJump(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** macOS takes ⌘ where the other systems take Ctrl. */
export const onMac = () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.userAgent);

/** The shortcut as the key's tooltip writes it, and as `aria-keyshortcuts` names it. */
export const shortcut = () => (onMac() ? { shown: '⌘K', aria: 'Meta+K' } : { shown: 'Ctrl+K', aria: 'Control+K' });

/** Whether a key press is the shortcut: K with Ctrl (⌘ on macOS) and nothing else. */
export const isShortcut = (event: KeyboardEvent) =>
  event.key.toLowerCase() === 'k' && !event.altKey && !event.shiftKey && (onMac() ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey);
