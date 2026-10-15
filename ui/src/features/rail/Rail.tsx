import { useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { PlotIcon } from '../../art/PlotIcon';
import { pixelUpscale } from '../../art/upscale';
import type { ServerStatus } from '../../ipc/schema';
import { useInstances, useInstanceArt, useSaveSettings, useServerStatus, useSettings } from '../../state/queries';
import { navigate, useRoute, instanceOf } from '../../state/route';
import { RAIL_POLL } from '../../state/serverPoll';
import { useRun } from '../../state/runStore';
import { Button } from '../../ui/Button';
import { ArrowBarLeft, ArrowBarRight, Plus, Search, WarningDiamond } from '../../ui/Icon';
import { sessionKind } from '../browse/session';
import { pickId } from '../home/pick';
import { statusWord } from '../servers/statusWord';

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
}: {
  item: Item;
  selected: boolean;
  /** The wide rail shows the name and the status word beside the icon. */
  wide: boolean;
  /** The one item Tab lands on: the selected one, or the first when none is selected. */
  entry: boolean;
  setRef: (el: HTMLButtonElement | null) => void;
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
      onClick={() => navigate({ name: 'home', id: item.id })}
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

/** Every instance as a cabinet icon; the selected one has the lit light. Arrow keys move along it.
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
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const items = useMemo<Item[]>(
    () => [
      ...(data?.instances ?? []).map((i) => ({ id: i.id, name: i.name, damaged: false, pack: i.source.kind !== 'none', server: i.source.kind === 'perland', lastStatus: i.server?.lastStatus ?? null, gone: i.server?.gone ?? false })),
      ...(data?.damaged ?? []).map((d) => ({ id: d.id, name: d.id, damaged: true, pack: false, server: false, lastStatus: null, gone: false })),
    ],
    [data],
  );
  const selected = route.name === 'home' || 'id' in route ? pickId(data, instanceOf(route)) : null;

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const buttons = refs.current.filter((b): b is HTMLButtonElement => b !== null);
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
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
      <ul className="rail-list" onKeyDown={onKeyDown}>
        {items.map((item, index) => (
          <li key={item.id}>
            <RailItem
              item={item}
              selected={item.id === selected}
              entry={selected ? item.id === selected : index === 0}
              wide={wide}
              setRef={(el) => {
                refs.current[index] = el;
              }}
            />
          </li>
        ))}
      </ul>
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
