import { useMemo, useRef, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { PlotIcon } from '../../art/PlotIcon';
import { useInstances } from '../../state/queries';
import { navigate, useRoute, instanceOf } from '../../state/route';
import { useRun } from '../../state/runStore';
import { Button } from '../../ui/Button';
import { Plus, WarningDiamond } from '../../ui/Icon';
import { pickId } from '../home/pick';

interface Item {
  id: string;
  name: string;
  damaged: boolean;
}

function RailItem({
  item,
  selected,
  entry,
  setRef,
}: {
  item: Item;
  selected: boolean;
  /** The one item Tab lands on: the selected one, or the first when none is selected. */
  entry: boolean;
  setRef: (el: HTMLButtonElement | null) => void;
}) {
  const { t } = useTranslation();
  const run = useRun(item.id);
  const status = item.damaged
    ? t('rail.damaged')
    : run.state === 'running' || run.state === 'stopping'
      ? t('rail.playing')
      : run.state === 'checking' || run.state === 'asking' || run.state === 'queued' || run.state === 'syncing' || run.state === 'preparing'
        ? t('rail.preparing')
        : null;
  return (
    <button
      ref={setRef}
      type="button"
      className="rail-item"
      data-selected={selected ? '' : undefined}
      aria-current={selected ? 'page' : undefined}
      aria-label={status ? `${item.name}, ${status}` : item.name}
      title={item.name}
      tabIndex={entry ? 0 : -1}
      onClick={() => navigate({ name: 'home', id: item.id })}
    >
      <span className={item.damaged ? 'opacity-40 grayscale' : undefined}>
        <PlotIcon seed={item.id} size={40} />
      </span>
      {status && !item.damaged && <span className="rail-led" aria-hidden />}
      {item.damaged && <WarningDiamond className="rail-warn" aria-hidden />}
    </button>
  );
}

/** Every instance as a cabinet icon; the selected one has the lit light. Arrow keys move along it. */
export function Rail() {
  const { t } = useTranslation();
  const { data } = useInstances();
  const route = useRoute();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const items = useMemo<Item[]>(
    () => [
      ...(data?.instances ?? []).map((i) => ({ id: i.id, name: i.name, damaged: false })),
      ...(data?.damaged ?? []).map((d) => ({ id: d.id, name: d.id, damaged: true })),
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
    <nav aria-label={t('rail.label')} className="flex min-h-0 flex-col items-center gap-3 bg-ground py-4">
      <ul className="flex min-h-0 flex-col items-center gap-3 overflow-y-auto px-3" onKeyDown={onKeyDown}>
        {items.map((item, index) => (
          <li key={item.id}>
            <RailItem
              item={item}
              selected={item.id === selected}
              entry={selected ? item.id === selected : index === 0}
              setRef={(el) => {
                refs.current[index] = el;
              }}
            />
          </li>
        ))}
      </ul>
      <Button variant="ghost" size="icon" className="btn-lg" aria-label={t('rail.newInstance')} title={t('rail.newInstance')} icon={<Plus />} onClick={() => navigate({ name: 'newInstance' })} />
    </nav>
  );
}
