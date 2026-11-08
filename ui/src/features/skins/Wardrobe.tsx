import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { commands } from '../../ipc/commands';
import type { Account, SkinEntry } from '../../ipc/schema';
import { AccountAvatar } from '../accounts/AccountAvatar';
import { Button } from '../../ui/Button';
import { Menu, type MenuHandle, type MenuItem } from '../../ui/Menu';
import { ChevronDown, Plus, Upload } from '../../ui/Icon';
import { dropIndex, preview } from '../rail/reorder';

/** How far the pointer travels before a press becomes a drag; anything less is a click. */
const DRAG_THRESHOLD = 4;

/** A press on a skin, from pointerdown until it is released or cancelled. */
interface Press {
  id: string;
  pointerId: number;
  x: number;
  y: number;
  /** Set once the pointer has travelled past the threshold. */
  carrying: boolean;
  /** Each skin's vertical midpoint when the drag began, top to bottom. */
  midpoints: number[];
  scrollTop: number;
}

interface Carry {
  id: string;
  from: number;
  /** How far the skin has travelled from its own place, in px. */
  lift: number;
  /** The slot (0..n) it would land in. */
  slot: number;
  midpoints: number[];
}

/** A skin's face, read straight off its texture: the head's front with the hat over it. */
function Face({ id, updated }: { id: string; updated: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const texture = useQuery({ queryKey: ['skinTexture', id, updated], queryFn: () => commands.skinTexture(id), staleTime: Infinity });
  useEffect(() => {
    const ctx = canvas.current?.getContext('2d');
    if (!ctx || !texture.data) return;
    const image = new Image();
    image.onload = () => {
      ctx.imageSmoothingEnabled = false;
      ctx.clearRect(0, 0, 8, 8);
      ctx.drawImage(image, 8, 8, 8, 8, 0, 0, 8, 8);
      ctx.drawImage(image, 40, 8, 8, 8, 0, 0, 8, 8);
    };
    image.src = texture.data;
  }, [texture.data]);
  return <canvas ref={canvas} width={8} height={8} className="wardrobe-face" aria-hidden />;
}

/** Whose wardrobe is shown: a key with the account's face and name that opens a menu of every account. */
function AccountSwitch({ accounts, owner, onOwner }: { accounts: Account[]; owner: Account; onOwner: (uuid: string) => void }) {
  const { t } = useTranslation();
  const menu = useRef<MenuHandle>(null);
  return (
    <>
      <Button
        className="wardrobe-owner"
        aria-haspopup="menu"
        aria-label={t('skins.wardrobeOf', { name: owner.username })}
        title={t('skins.wardrobeOf', { name: owner.username })}
        icon={<AccountAvatar account={owner} size={20} />}
        onClick={(e) => menu.current?.openAt(e.currentTarget)}
      >
        <span className="min-w-0 flex-1 truncate text-left">{owner.username}</span>
        <ChevronDown aria-hidden />
      </Button>
      <Menu
        ref={menu}
        label={t('skins.switchAccount')}
        trigger={false}
        className="contents"
        items={accounts.map((a) => ({
          id: a.uuid,
          label: a.username,
          icon: a.uuid === owner.uuid ? <span className="lamp" aria-hidden /> : <AccountAvatar account={a} size={16} />,
          onSelect: () => onOwner(a.uuid),
        }))}
      />
    </>
  );
}

export function Wardrobe({
  accounts,
  owner,
  onOwner,
  entries,
  selected,
  wearer,
  onSelect,
  emptyNote,
  newItems,
  onImport,
  menuFor,
  onMove,
}: {
  /** Every account, for the switch; it's left out while none is signed in. */
  accounts: Account[];
  /** The account whose wardrobe this is (null: the skins made with no account). */
  owner: Account | null;
  onOwner: (uuid: string) => void;
  entries: SkinEntry[];
  selected: string | null;
  /** The account whose worn skin gets the lamp. */
  wearer: string | null;
  onSelect: (id: string) => void;
  /** What an empty wardrobe says. */
  emptyNote: string;
  newItems: MenuItem[];
  onImport: () => void;
  menuFor: (entry: SkinEntry) => MenuItem[];
  /** Moves the skin at index `from` to index `to` in the wardrobe's order. */
  onMove: (from: number, to: number) => void;
}) {
  const { t } = useTranslation();
  const newMenu = useRef<MenuHandle>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const picks = useRef(new Map<string, HTMLButtonElement>());
  const press = useRef<Press | null>(null);
  /** True (once) when a press ended a drag, so the click that follows doesn't also open the skin. */
  const endedDrag = useRef(false);
  /** The skin that just moved by keyboard: focus goes back to it if the move took it away. */
  const follow = useRef<string | null>(null);
  const [carry, setCarry] = useState<Carry | null>(null);
  const [announcement, setAnnouncement] = useState('');
  const ids = entries.map((e) => e.id);

  // A moved row's node is taken out and put back, which drops focus; hand it back, once.
  useLayoutEffect(() => {
    const id = follow.current;
    const pick = id ? picks.current.get(id) : undefined;
    if (pick && document.activeElement !== pick && (document.activeElement === null || document.activeElement === document.body)) pick.focus();
    follow.current = null;
  }, [entries]);

  const move = (from: number, to: number) => {
    const entry = entries[from];
    if (!entry || to === from || to < 0 || to >= entries.length) {
      follow.current = null;
      return;
    }
    onMove(from, to);
    setAnnouncement(t('skins.moved', { name: entry.name, position: to + 1, count: entries.length }));
  };

  const stopCarrying = () => {
    press.current = null;
    setCarry(null);
  };

  const scrolledBy = (p: Press) => (listRef.current?.scrollTop ?? 0) - p.scrollTop;

  const grip = (id: string) => ({
    onPointerDown: (event: PointerEvent<HTMLButtonElement>) => {
      endedDrag.current = false;
      follow.current = null;
      if (event.button !== 0) return;
      press.current = { id, pointerId: event.pointerId, x: event.clientX, y: event.clientY, carrying: false, midpoints: [], scrollTop: 0 };
      try {
        event.currentTarget.setPointerCapture(event.pointerId);
      } catch {
        // No capture (an unknown pointer); the drag still works while the pointer stays over the list.
      }
    },
    onPointerMove: (event: PointerEvent<HTMLButtonElement>) => {
      const p = press.current;
      if (!p || p.id !== id || p.pointerId !== event.pointerId) return;
      if (!p.carrying) {
        if (Math.hypot(event.clientX - p.x, event.clientY - p.y) < DRAG_THRESHOLD) return;
        p.carrying = true;
        p.midpoints = ids.map((k) => {
          const rect = picks.current.get(k)?.getBoundingClientRect();
          return rect ? rect.top + rect.height / 2 : 0;
        });
        p.scrollTop = listRef.current?.scrollTop ?? 0;
      }
      const from = ids.indexOf(id);
      if (from < 0) return;
      // Scrolling the list mid-drag shifts every row; measure in the list's own coordinates.
      const y = event.clientY + scrolledBy(p);
      const own = p.midpoints[from] ?? p.y;
      // The carried skin stays between the first and last places.
      const lift = Math.min(Math.max(y - p.y, (p.midpoints[0] ?? own) - own), (p.midpoints.at(-1) ?? own) - own);
      setCarry({ id, from, lift, slot: dropIndex(p.midpoints, y), midpoints: p.midpoints });
    },
    onPointerUp: (event: PointerEvent<HTMLButtonElement>) => {
      const p = press.current;
      if (!p || p.pointerId !== event.pointerId) return;
      if (p.carrying) {
        endedDrag.current = true;
        follow.current = id;
        const slot = dropIndex(p.midpoints, event.clientY + scrolledBy(p));
        // The list may have changed under the drag: move this skin by its id, from where it is now.
        const from = ids.indexOf(p.id);
        if (from >= 0) move(from, slot > from ? slot - 1 : slot);
      }
      stopCarrying();
    },
    onPointerCancel: stopCarrying,
    onLostPointerCapture: (event: PointerEvent<HTMLButtonElement>) => {
      // Released normally, pointerup has already cleared the press; otherwise the drag is abandoned.
      if (press.current?.pointerId === event.pointerId) {
        if (press.current.carrying) endedDrag.current = true;
        stopCarrying();
      }
    },
  });

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key === 'Escape' && press.current?.carrying) {
      // Put it back; the Escape is the drag's alone.
      event.preventDefault();
      event.stopPropagation();
      endedDrag.current = true;
      stopCarrying();
      return;
    }
    if (!event.altKey || (event.key !== 'ArrowUp' && event.key !== 'ArrowDown')) return;
    // Alt+arrows move the focused skin one place.
    const id = [...picks.current].find(([, el]) => el === document.activeElement)?.[0];
    const from = id === undefined ? -1 : ids.indexOf(id);
    if (from < 0) return;
    event.preventDefault();
    follow.current = id ?? null;
    move(from, from + (event.key === 'ArrowDown' ? 1 : -1));
  };
  return (
    <aside className="wardrobe" aria-labelledby="wardrobe-title" data-switch={owner ? true : undefined}>
      {owner && <AccountSwitch accounts={accounts} owner={owner} onOwner={onOwner} />}
      <div className="flex items-center justify-between gap-2">
        <h2 id="wardrobe-title" className="font-bold">
          {t('skins.wardrobe')}
        </h2>
        <span className="text-sm text-ink-dim tabular">{entries.length}</span>
      </div>
      <div className="wardrobe-actions">
        <Button icon={<Plus />} aria-haspopup="menu" onClick={(e) => newMenu.current?.openAt(e.currentTarget)}>
          {t('skins.new')}
        </Button>
        <Button icon={<Upload />} onClick={onImport}>
          {t('skins.import')}
        </Button>
      </div>
      <Menu ref={newMenu} label={t('skins.new')} items={newItems} trigger={false} className="contents" />
      {entries.length === 0 ? (
        <p className="text-sm text-ink-dim">{emptyNote}</p>
      ) : (
        <ul ref={listRef} className="wardrobe-list" aria-label={t('skins.wardrobe')} aria-describedby="wardrobe-reorder-hint" onKeyDown={onKeyDown}>
          {entries.map((entry, order) => {
            const isSelected = entry.id === selected;
            const worn = wearer !== null && entry.wornBy === wearer;
            const held = carry?.id === entry.id;
            const place = carry ? preview(carry, order) : null;
            return (
              <li
                key={entry.id}
                className="wardrobe-item"
                data-selected={isSelected || undefined}
                data-carry={held ? '' : undefined}
                data-drop={held && place?.moved ? '' : undefined}
                data-shift={!held && place?.shift ? '' : undefined}
                style={
                  held
                    ? ({ '--socket': `${place?.socket ?? 0}px`, '--lift': `${carry.lift}px` } as CSSProperties)
                    : place?.shift
                      ? ({ '--shift': `${place.shift}px` } as CSSProperties)
                      : undefined
                }
              >
                <button
                  ref={(el) => {
                    if (el) picks.current.set(entry.id, el);
                    else picks.current.delete(entry.id);
                  }}
                  type="button"
                  className="wardrobe-pick"
                  aria-current={isSelected || undefined}
                  {...grip(entry.id)}
                  onClick={() => {
                    if (endedDrag.current) {
                      endedDrag.current = false;
                      return;
                    }
                    onSelect(entry.id);
                  }}
                >
                  <Face id={entry.id} updated={entry.updated} />
                  <span className="grid min-w-0 text-left">
                    <span className="truncate font-semibold">{entry.name}</span>
                    <span className="flex items-center gap-1.5 text-xs text-ink-dim">
                      {worn && <span className="lamp" aria-hidden />}
                      {worn ? t('skins.wearing') : t(`skins.model.${entry.model}`)}
                    </span>
                  </span>
                </button>
                <Menu label={t('skins.moreFor', { name: entry.name })} items={menuFor(entry)} compact floating />
              </li>
            );
          })}
        </ul>
      )}
      <p id="wardrobe-reorder-hint" className="sr-only">
        {t('skins.reorderHint')}
      </p>
      <p className="sr-only" aria-live="polite">
        {announcement}
      </p>
    </aside>
  );
}
