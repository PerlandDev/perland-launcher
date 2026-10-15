import {
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent,
} from 'react';
import { useTranslation } from 'react-i18next';
import { PlotIcon } from '../../art/PlotIcon';
import { pixelUpscale } from '../../art/upscale';
import type { ServerStatus } from '../../ipc/schema';
import { useInstances, useInstanceArt, useReorderInstances, useSaveSettings, useServerStatus, useSettings } from '../../state/queries';
import { navigate, rememberInstance, useRoute, instanceOf } from '../../state/route';
import { RAIL_POLL } from '../../state/serverPoll';
import { useRun } from '../../state/runStore';
import { Button } from '../../ui/Button';
import { ArrowBarLeft, ArrowBarRight, Plus, Search, WarningDiamond } from '../../ui/Icon';
import { sessionKind } from '../browse/session';
import { pickId } from '../home/pick';
import { statusWord } from '../servers/statusWord';
import { dropIndex, moveItem } from './reorder';

/** How far the pointer travels before a press becomes a drag; anything less is a click. */
const DRAG_THRESHOLD = 4;

/** The pointer handlers that pick a cabinet up and carry it. Damaged cabinets get none. */
type Grip = Pick<
  ButtonHTMLAttributes<HTMLButtonElement>,
  'onPointerDown' | 'onPointerMove' | 'onPointerUp' | 'onPointerCancel' | 'onLostPointerCapture'
>;

/** A press on a cabinet, from pointerdown until it is released or cancelled. */
interface Press {
  id: string;
  from: number;
  pointerId: number;
  x: number;
  y: number;
  /** Set once the pointer has travelled past the threshold. */
  carrying: boolean;
  /** Each healthy cabinet's vertical midpoint when the drag began, top to bottom. */
  midpoints: number[];
  scrollTop: number;
}

interface Carry {
  id: string;
  from: number;
  /** How far the cabinet has travelled from its own place, in px. */
  lift: number;
  /** The slot (0..n) it would land in. */
  slot: number;
}

interface Item {
  id: string;
  name: string;
  damaged: boolean;
  pack: boolean;
  server: boolean;
  lastStatus: ServerStatus | null;
  gone: boolean;
}

function RailItem({
  item,
  selected,
  entry,
  wide,
  setRef,
  grip,
  lift,
  movable,
  endedDrag,
}: {
  item: Item;
  selected: boolean;
  /** The wide rail shows the name and the status word beside the icon. */
  wide: boolean;
  /** The one item Tab lands on: the selected one, or the first when none is selected. */
  entry: boolean;
  setRef: (el: HTMLButtonElement | null) => void;
  grip: Grip | undefined;
  /** While this cabinet is carried, how far it has travelled from its place in px; otherwise null. */
  lift: number | null;
  /** A healthy instance, which Alt+arrows can move. */
  movable: boolean;
  /** True (once) when this press ended a drag, so the click that follows doesn't also select. */
  endedDrag: () => boolean;
}) {
  const { t } = useTranslation();
  const run = useRun(item.id);
  const art = useInstanceArt(item.id);
  // An icon that fails to decode gives the cabinet back its generated plot icon.
  const [failed, setFailed] = useState<string | null>(null);
  const icon = art.data?.icon && art.data.icon !== failed ? art.data.icon : null;
  // A small pixel icon blown up to the 40px slot keeps hard pixels; a big icon shrinks smoothly.
  const [upscaled, setUpscaled] = useState<string | null>(null);
  const status = item.damaged
    ? t('rail.damaged')
    : run.state === 'running' || run.state === 'stopping'
      ? t('rail.playing')
      : run.state === 'checking' || run.state === 'asking' || run.state === 'queued' || run.state === 'syncing' || run.state === 'preparing'
        ? t('rail.preparing')
        : null;
  const live = useServerStatus(item.id, item.server, RAIL_POLL);
  const serverStatus = live.data ? live.data.status : item.lastStatus;
  const serverGone = live.data?.gone ?? item.gone;
  const serverWord = item.server ? statusWord(t, serverStatus, live.data?.players ?? null, serverGone) : null;
  const label = [item.name, status, serverWord].filter(Boolean).join(', ');
  const word = [status, serverWord].filter(Boolean).join(' · ');
  return (
    <button
      ref={setRef}
      type="button"
      className="rail-item"
      data-selected={selected ? '' : undefined}
      aria-current={selected ? 'page' : undefined}
      aria-label={label}
      title={[item.name, serverWord].filter(Boolean).join(' · ')}
      tabIndex={entry ? 0 : -1}
      aria-keyshortcuts={movable ? 'Alt+ArrowUp Alt+ArrowDown' : undefined}
      data-dragging={lift !== null ? '' : undefined}
      style={lift !== null ? ({ '--lift': `${lift}px` } as CSSProperties) : undefined}
      {...grip}
      onClick={() => {
        if (!endedDrag()) navigate({ name: 'home', id: item.id });
      }}
    >
      <span className="rail-icon">
        <span className={item.damaged ? 'opacity-40 grayscale' : undefined}>
          {icon ? (
            <img
              src={icon}
              alt=""
              width={40}
              height={40}
              className="rail-art"
              draggable={false}
              data-upscale={upscaled === icon ? '' : undefined}
              onLoad={(e) => {
                const { naturalWidth: width, naturalHeight: height } = e.currentTarget;
                setUpscaled(pixelUpscale({ width, height }, { width: 40, height: 40 }) ? icon : null);
              }}
              onError={() => setFailed(icon)}
            />
          ) : (
            <PlotIcon seed={item.id} size={40} />
          )}
        </span>
        {status && !item.damaged && <span className="rail-led" aria-hidden />}
        {item.server && <span className="rail-server" data-status={serverGone ? 'unknown' : (serverStatus ?? 'unknown')} aria-hidden />}
        {item.damaged && <WarningDiamond className="rail-warn" aria-hidden />}
      </span>
      {wide && (
        <span className="rail-text">
          <span className="rail-name" title={item.name}>
            {item.name}
          </span>
          {word && <span className="rail-word">{word}</span>}
        </span>
      )}
    </button>
  );
}

/** Where the drop line shows while a cabinet is carried: above the cabinet at `order`, or below the
 *  last healthy one. Nothing at the two slots either side of the carried cabinet, which leave it where it is. */
function dropMark(carry: Carry, order: number, count: number): 'before' | 'after' | undefined {
  if (order < 0 || carry.slot === carry.from || carry.slot === carry.from + 1) return undefined;
  if (carry.slot === order) return 'before';
  if (carry.slot === count && order === count - 1) return 'after';
  return undefined;
}

/** Every instance as a cabinet icon; the selected one has the lit light. Arrow keys move along it,
 *  and Alt+arrows or a drag move a cabinet to a new place in the player's order.
 *  Its key at the top widens it to show each name and status word, and the settings remember that. */
export function Rail() {
  const { t } = useTranslation();
  const { data } = useInstances();
  const settings = useSettings();
  const save = useSaveSettings();
  const wide = settings.data?.railExpanded ?? false;
  const toggleWide = () => {
    if (settings.data) save.mutate({ ...settings.data, railExpanded: !wide });
  };
  const route = useRoute();
  const reorder = useReorderInstances();
  const refs = useRef(new Map<string, HTMLButtonElement>());
  const listRef = useRef<HTMLUListElement>(null);
  const press = useRef<Press | null>(null);
  const endedDrag = useRef(false);
  /** The cabinet that just moved by keyboard; focus goes back to it if the move took it away. */
  const follow = useRef<string | null>(null);
  const [carry, setCarry] = useState<Carry | null>(null);
  const [announcement, setAnnouncement] = useState('');
  const items = useMemo<Item[]>(
    () => [
      ...(data?.instances ?? []).map((i) => ({ id: i.id, name: i.name, damaged: false, pack: i.source.kind !== 'none', server: i.source.kind === 'perland', lastStatus: i.server?.lastStatus ?? null, gone: i.server?.gone ?? false })),
      ...(data?.damaged ?? []).map((d) => ({ id: d.id, name: d.id, damaged: true, pack: false, server: false, lastStatus: null, gone: false })),
    ],
    [data],
  );
  const selected = route.name === 'home' || 'id' in route ? pickId(data, instanceOf(route)) : null;

  // Only healthy instances have a place in the order; damaged ones always sit last and never move.
  const ids = useMemo(() => (data?.instances ?? []).map((i) => i.id), [data]);

  // A moved cabinet's node is taken out and put back, which drops focus; hand it back.
  useLayoutEffect(() => {
    const id = follow.current;
    const button = id ? refs.current.get(id) : undefined;
    if (button && document.activeElement !== button && (document.activeElement === null || document.activeElement === document.body)) {
      button.focus();
    }
  }, [items]);

  const move = (from: number, to: number) => {
    const id = ids[from];
    if (id === undefined || to === from || to < 0 || to >= ids.length) return;
    // With nothing chosen yet, Home shows the first instance; pin the one on screen so a move doesn't swap it.
    if (selected && !instanceOf(route)) rememberInstance(selected);
    reorder.mutate(moveItem(ids, from, to));
    const name = items[from]?.name ?? id;
    setAnnouncement(t('rail.moved', { name, position: to + 1, count: ids.length }));
  };

  const midpointsNow = () =>
    ids.map((id) => {
      const rect = refs.current.get(id)?.getBoundingClientRect();
      return rect ? rect.top + rect.height / 2 : 0;
    });

  const stopCarrying = () => {
    press.current = null;
    setCarry(null);
  };

  const gripFor = (id: string, from: number): Grip => ({
    onPointerDown: (event: PointerEvent<HTMLButtonElement>) => {
      endedDrag.current = false;
      follow.current = null;
      if (event.button !== 0) return;
      press.current = {
        id,
        from,
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        carrying: false,
        midpoints: [],
        scrollTop: 0,
      };
      try {
        event.currentTarget.setPointerCapture(event.pointerId);
      } catch {
        // No capture (an unknown pointer); the drag still works while the pointer stays over the rail.
      }
    },
    onPointerMove: (event: PointerEvent<HTMLButtonElement>) => {
      const p = press.current;
      if (!p || p.id !== id || p.pointerId !== event.pointerId) return;
      if (!p.carrying) {
        if (Math.hypot(event.clientX - p.x, event.clientY - p.y) < DRAG_THRESHOLD) return;
        p.carrying = true;
        p.midpoints = midpointsNow();
        p.scrollTop = listRef.current?.scrollTop ?? 0;
      }
      // Scrolling the list mid-drag shifts every cabinet; measure in the list's own coordinates.
      const scrolled = (listRef.current?.scrollTop ?? 0) - p.scrollTop;
      const y = event.clientY + scrolled;
      const own = p.midpoints[p.from] ?? p.y;
      // The carried cabinet stays between the first and last healthy places, never over the damaged ones or the keys.
      const lift = Math.min(Math.max(y - p.y, (p.midpoints[0] ?? own) - own), (p.midpoints.at(-1) ?? own) - own);
      setCarry({ id, from: p.from, lift, slot: dropIndex(p.midpoints, y) });
    },
    onPointerUp: (event: PointerEvent<HTMLButtonElement>) => {
      const p = press.current;
      if (!p || p.pointerId !== event.pointerId) return;
      if (p.carrying) {
        endedDrag.current = true;
        follow.current = id;
        const scrolled = (listRef.current?.scrollTop ?? 0) - p.scrollTop;
        const slot = dropIndex(p.midpoints, event.clientY + scrolled);
        move(p.from, slot > p.from ? slot - 1 : slot);
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
      // Put it back. The release that follows is still not a click.
      event.preventDefault();
      endedDrag.current = true;
      stopCarrying();
      return;
    }
    const buttons = items.map((i) => refs.current.get(i.id)).filter((b): b is HTMLButtonElement => b !== undefined);
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.altKey) {
      // Alt+arrows move the focused cabinet one place; they never also step focus along the rail.
      if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
      event.preventDefault();
      const id = items[index]?.id;
      const from = id === undefined ? -1 : ids.indexOf(id);
      if (from < 0) return;
      follow.current = id ?? null;
      move(from, from + (event.key === 'ArrowDown' ? 1 : -1));
      return;
    }
    follow.current = null;
    const next =
      event.key === 'ArrowDown' ? index + 1 : event.key === 'ArrowUp' ? index - 1 : event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : null;
    if (next === null || buttons.length === 0) return;
    event.preventDefault();
    buttons[(next + buttons.length) % buttons.length]?.focus();
  };

  return (
    <nav aria-label={t('rail.label')} className="rail" data-wide={wide ? '' : undefined}>
      <Button
        variant="ghost"
        size={wide ? 'md' : 'icon'}
        className="rail-key"
        aria-pressed={wide}
        aria-label={wide ? t('rail.collapse') : t('rail.expand')}
        title={wide ? t('rail.collapse') : t('rail.expand')}
        icon={wide ? <ArrowBarLeft /> : <ArrowBarRight />}
        onClick={toggleWide}
      >
        {wide && t('rail.collapse')}
      </Button>
      <ul ref={listRef} className="rail-list" onKeyDown={onKeyDown}>
        {items.map((item, index) => {
          const order = ids.indexOf(item.id);
          return (
            <li key={item.id} data-drop={carry ? dropMark(carry, order, ids.length) : undefined}>
              <RailItem
                item={item}
                selected={item.id === selected}
                entry={selected ? item.id === selected : index === 0}
                wide={wide}
                setRef={(el) => {
                  if (el) refs.current.set(item.id, el);
                  else refs.current.delete(item.id);
                }}
                grip={order >= 0 ? gripFor(item.id, order) : undefined}
                lift={carry?.id === item.id ? carry.lift : null}
                movable={order >= 0}
                endedDrag={() => {
                  const ended = endedDrag.current;
                  endedDrag.current = false;
                  return ended;
                }}
              />
            </li>
          );
        })}
      </ul>
      <p className="sr-only" aria-live="polite">
        {announcement}
      </p>
      <Button
        size={wide ? 'md' : 'icon'}
        className="btn-lg rail-browse rail-foot"
        aria-label={t('browse.open')}
        title={t('browse.open')}
        aria-current={route.name === 'browse' && !route.instance ? 'page' : undefined}
        icon={<Search />}
        onClick={() => navigate({ name: 'browse', kind: sessionKind() })}
      >
        {wide && t('browse.open')}
      </Button>
      <Button
        variant="ghost"
        size={wide ? 'md' : 'icon'}
        className="btn-lg rail-foot"
        aria-label={t('rail.newInstance')}
        title={t('rail.newInstance')}
        icon={<Plus />}
        onClick={() => navigate({ name: 'newInstance' })}
      >
        {wide && t('rail.newInstance')}
      </Button>
    </nav>
  );
}
