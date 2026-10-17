import { useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode, type Ref } from 'react';
import { createPortal } from 'react-dom';
import { Button } from './Button';
import { MoreHorizontal } from './Icon';

export interface MenuItem {
  /** Stable across languages; the label is the fallback key. */
  id?: string;
  label: string;
  onSelect: () => void;
  icon?: ReactNode;
  danger?: boolean;
  /** Shown but not chosen, and skipped by the arrow keys. */
  disabled?: boolean;
}

/** Opens the menu from outside its key: at a right-click's point, or beside an element (the keyboard's menu key). */
export interface MenuHandle {
  openAt(at: { x: number; y: number } | Element): void;
}

/** What a floating menu opens beside: a point (top equals bottom) or an element's box. */
interface Anchor {
  left: number;
  top: number;
  bottom: number;
}

/** The room a floating menu keeps from the window's edges, and its gap from an element it opens beside, in px. */
const EDGE = 8;

const ENABLED_ITEM = '[role="menuitem"]:not(:disabled)';

/** `compact` is a card's quiet ghost key that opens the menu below it; the default is a large raised key opening above.
 *  `floating` opens it in the window's own layer beside its key, so a scrolling list never clips it, and a menu opened
 *  through `ref` (a right-click) always floats. `trigger={false}` drops the key: then only `ref` opens it. */
export function Menu({
  label,
  items,
  compact = false,
  floating = false,
  trigger: withTrigger = true,
  triggerTabIndex,
  className = 'relative',
  ref,
}: {
  label: string;
  items: MenuItem[];
  compact?: boolean;
  floating?: boolean;
  trigger?: boolean;
  triggerTabIndex?: number;
  className?: string;
  ref?: Ref<MenuHandle>;
}) {
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  /** Where focus goes back on close: the key, or whatever had it when a right-click opened the menu. */
  const returnTo = useRef<HTMLElement | null>(null);

  const show = (at: Anchor | null, from: HTMLElement | null) => {
    returnTo.current = from;
    setAnchor(at);
    setPlace(null);
    setOpen(true);
  };
  const boxOf = (el: Element): Anchor => {
    const rect = el.getBoundingClientRect();
    return { left: rect.left, top: rect.top, bottom: rect.bottom };
  };

  useImperativeHandle(ref, () => ({
    openAt(at) {
      const focused = document.activeElement instanceof HTMLElement && document.activeElement !== document.body ? document.activeElement : null;
      if (at instanceof Element) show(boxOf(at), at instanceof HTMLElement ? at : focused);
      else show({ left: at.x, top: at.y, bottom: at.y }, focused);
    },
  }));

  // A floating menu opens below its anchor, and flips above it or slides left rather than leave the window.
  useLayoutEffect(() => {
    if (!open || !anchor || place || !list.current) return;
    const { width, height } = list.current.getBoundingClientRect();
    const gap = anchor.bottom > anchor.top ? EDGE : 0;
    const below = anchor.bottom + gap;
    const top = below + height > window.innerHeight - EDGE ? Math.max(EDGE, anchor.top - gap - height) : below;
    const left = Math.max(EDGE, Math.min(anchor.left, window.innerWidth - EDGE - width));
    setPlace({ left, top });
  }, [open, anchor, place]);

  useEffect(() => {
    if (!open) return;
    list.current?.querySelector<HTMLElement>(ENABLED_ITEM)?.focus();
    const onPointer = (event: PointerEvent) => {
      if (!list.current?.contains(event.target as Node) && !trigger.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', onPointer);
    return () => document.removeEventListener('pointerdown', onPointer);
  }, [open]);

  const close = () => {
    setOpen(false);
    const back = returnTo.current;
    (back?.isConnected ? back : trigger.current)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // A floating menu is portalled, but React still bubbles its keys to the list it opened from (the rail's arrows).
    event.stopPropagation();
    const entries = [...(list.current?.querySelectorAll<HTMLElement>(ENABLED_ITEM) ?? [])];
    const index = entries.indexOf(document.activeElement as HTMLElement);
    if (event.key === 'Escape') {
      event.preventDefault();
      close();
    } else if (event.key === 'Tab') {
      close();
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      entries[(index + 1) % entries.length]?.focus();
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      entries[(index - 1 + entries.length) % entries.length]?.focus();
    }
  };

  const popup = open && (
    <div
      ref={list}
      role="menu"
      aria-label={label}
      className="menu"
      data-placement={anchor ? 'float' : compact ? 'down' : undefined}
      // Measured unseen for one layout pass, then placed, so it never paints in the wrong spot.
      style={anchor ? { left: place?.left ?? 0, top: place?.top ?? 0, visibility: place ? undefined : 'hidden' } : undefined}
      onKeyDown={onKeyDown}
      // A right-click on the menu itself must not reopen it from whatever it floats over.
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      {items.map((item) => (
        <button
          key={item.id ?? item.label}
          type="button"
          role="menuitem"
          disabled={item.disabled}
          data-danger={item.danger ? '' : undefined}
          onClick={() => {
            close();
            item.onSelect();
          }}
        >
          {item.icon}
          {item.label}
        </button>
      ))}
    </div>
  );

  return (
    <div className={className}>
      {withTrigger && (
        <Button
          ref={trigger}
          size="icon"
          variant={compact ? 'ghost' : 'secondary'}
          className={compact ? undefined : 'btn-lg'}
          aria-label={label}
          aria-haspopup="menu"
          aria-expanded={open}
          title={label}
          tabIndex={triggerTabIndex}
          icon={<MoreHorizontal />}
          onClick={(event) => {
            if (open) setOpen(false);
            else show(floating ? boxOf(event.currentTarget) : null, event.currentTarget);
          }}
        />
      )}
      {popup && (anchor ? createPortal(popup, document.body) : popup)}
    </div>
  );
}
