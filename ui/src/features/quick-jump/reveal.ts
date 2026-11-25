// Where Go to puts the keyboard once it has moved the player. A field is found on its screen by the label the screen
// shows for it (the same i18n key Go to lists it by), so the screens need no hooks of their own and a field found in
// English is found in Spanish. The tests in reveal.test.tsx render the real screens and fail if a label stops matching.

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
/** Elements that can carry a field's or a section's label. */
const LABELS = 'label, legend, h1, h2, h3, p, span, button, [aria-label]';

/** The first element in `root` labelled `text`: by its own words, or by its accessible label. */
function labelled(root: ParentNode, text: string): HTMLElement | null {
  for (const el of root.querySelectorAll<HTMLElement>(LABELS)) {
    if (el.getAttribute('aria-label') === text || el.textContent?.trim() === text) return el;
  }
  return null;
}

/** The control a label belongs to: its own control, the key it sits in, else the first one in the smallest block
 *  around it that has any (a chosen option first, so the arrow keys go on from there). */
function controlFor(label: HTMLElement): { control: HTMLElement | null; block: HTMLElement } {
  if (label instanceof HTMLLabelElement) {
    const control = label.control as HTMLElement | null;
    if (control && !(control as HTMLInputElement).disabled) return { control, block: label.parentElement ?? label };
  }
  if (label.matches(FOCUSABLE)) return { control: label, block: label };
  const key = label.closest<HTMLElement>('button:not([disabled])');
  if (key) return { control: key, block: key };
  for (let block = label.parentElement; block && block.tagName !== 'MAIN'; block = block.parentElement) {
    const chosen = block.querySelector<HTMLElement>('input:checked:not([disabled])');
    const first = chosen ?? block.querySelector<HTMLElement>(FOCUSABLE);
    if (first) return { control: first, block };
  }
  return { control: null, block: label };
}

function focusOn(el: HTMLElement, block: HTMLElement) {
  block.scrollIntoView?.({ block: 'center' });
  el.focus({ preventScroll: true });
}

/** Once `find()` answers (now, or as the screen renders), call `then`; `false` after `timeout` ms without one. */
function whenFound<T>(find: () => T | null, then: (found: T) => void, timeout: number): Promise<boolean> {
  return new Promise((resolve) => {
    // A move renders on the next turn, so the screen being left is never the one searched.
    setTimeout(() => {
      const now = find();
      if (now) {
        then(now);
        resolve(true);
        return;
      }
      const observer = new MutationObserver(() => {
        const found = find();
        if (!found) return;
        done();
        then(found);
        resolve(true);
      });
      const timer = setTimeout(() => {
        done();
        resolve(false);
      }, timeout);
      const done = () => {
        observer.disconnect();
        clearTimeout(timer);
      };
      observer.observe(document.body, { childList: true, subtree: true });
    }, 0);
  });
}

const screenRoot = (): ParentNode => document.querySelector('main') ?? document.body;

/** Scroll the field labelled `text` into view and put the keyboard on it. A field that can't take focus (a locked
 *  choice) has its label focused instead, so the player still lands where it is. */
export function reveal(text: string, timeout = 3000): Promise<boolean> {
  return whenFound(
    () => labelled(screenRoot(), text),
    (label) => {
      const { control, block } = controlFor(label);
      if (control) return focusOn(control, block);
      label.tabIndex = -1;
      focusOn(label, label);
    },
    timeout,
  );
}

/** After a move to a screen: the keyboard on its title, so a screen reader says where the player is. */
export function land(timeout = 3000): Promise<boolean> {
  return whenFound(
    () => screenRoot().querySelector<HTMLElement>('h1'),
    (title) => {
      title.tabIndex = -1;
      title.dataset.landing = '';
      title.focus({ preventScroll: true });
    },
    timeout,
  );
}
