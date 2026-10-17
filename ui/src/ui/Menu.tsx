import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { Button } from './Button';
import { MoreHorizontal } from './Icon';

export interface MenuItem {
  label: string;
  onSelect: () => void;
  icon?: ReactNode;
  danger?: boolean;
  /** Shown but not chosen, and skipped by the arrow keys. */
  disabled?: boolean;
}

const ENABLED_ITEM = '[role="menuitem"]:not(:disabled)';

/** `compact` is a card's quiet ghost key that opens the menu below it; the default is a large raised key opening above. */
export function Menu({ label, items, compact = false }: { label: string; items: MenuItem[]; compact?: boolean }) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);

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
    trigger.current?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
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

  return (
    <div className="relative">
      <Button
        ref={trigger}
        size="icon"
        variant={compact ? 'ghost' : 'secondary'}
        className={compact ? undefined : 'btn-lg'}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        icon={<MoreHorizontal />}
        onClick={() => setOpen((v) => !v)}
      />
      {open && (
        <div ref={list} role="menu" aria-label={label} className="menu" data-placement={compact ? 'down' : undefined} onKeyDown={onKeyDown}>
          {items.map((item) => (
            <button
              key={item.label}
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
      )}
    </div>
  );
}
