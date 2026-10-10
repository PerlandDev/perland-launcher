import { useVirtualizer } from '@tanstack/react-virtual';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../ui/Button';

/** The game's output, virtualized; follows the newest line until the player scrolls up. */
export function LogView({ lines }: { lines: string[] }) {
  const { t } = useTranslation();
  const scroller = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual returns new functions each render by design
  const virtualizer = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 20,
    overscan: 30,
    initialRect: { width: 800, height: 320 },
  });

  useEffect(() => {
    if (follow && lines.length > 0) virtualizer.scrollToIndex(lines.length - 1, { align: 'end' });
  }, [follow, lines.length, virtualizer]);

  if (lines.length === 0) {
    return <p className="p-4 text-ink-dim">{t('activity.logEmpty')}</p>;
  }

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={scroller}
        role="log"
        aria-label={t('activity.log')}
        className="selectable h-full overflow-auto bg-ground py-2 font-mono text-[13px] leading-5"
        onScroll={(event) => {
          const el = event.currentTarget;
          setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 40);
        }}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualizer.getVirtualItems().map((row) => (
            <div
              key={row.key}
              className="absolute left-0 w-full truncate px-4 whitespace-pre"
              style={{ transform: `translateY(${row.start}px)`, height: row.size }}
            >
              {lines[row.index]}
            </div>
          ))}
        </div>
      </div>
      {!follow && (
        <Button className="absolute right-4 bottom-4" onClick={() => setFollow(true)}>
          {t('activity.jumpToEnd')}
        </Button>
      )}
    </div>
  );
}
