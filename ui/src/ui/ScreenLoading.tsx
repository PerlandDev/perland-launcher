import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { Meter } from './Meter';

/** A load faster than this shows nothing at all, so it never flickers. */
export const SHOW_AFTER_MS = 250;
/** After this long the screen says so and offers to try again. */
export const SLOW_AFTER_MS = 8000;

/** What a screen shows while it is on its way: nothing for a moment, then Play's checking meter (one blue block
 *  sweeping a recessed channel; under reduced motion the empty channel) over a plain line of what is loading, and,
 *  when it is slow, a dim line saying so with Try again. `center` stands it in the middle of its area (a screen with
 *  no column); `start` sets it at the top left of a screen's column, where the content will be. */
export function ScreenLoading({ label, onRetry, place = 'start' }: { label: string; onRetry?: () => void; place?: 'center' | 'start' }) {
  const { t } = useTranslation();
  const [shown, setShown] = useState(false);
  const [slow, setSlow] = useState(false);
  // Each try starts the clock for the slow line again.
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const show = setTimeout(() => setShown(true), SHOW_AFTER_MS);
    return () => clearTimeout(show);
  }, []);
  useEffect(() => {
    const later = setTimeout(() => setSlow(true), SLOW_AFTER_MS);
    return () => clearTimeout(later);
  }, [attempt]);
  const retry = () => {
    setSlow(false);
    setAttempt((n) => n + 1);
    onRetry?.();
  };
  return (
    <div className="screen-loading" data-place={place} aria-busy="true">
      {shown && <Meter value={0} label={label} scan channel decorative />}
      <div role="status" className="grid gap-1">
        {shown && <p className="font-semibold">{label}</p>}
        {shown && slow && <p className="text-sm text-ink-dim">{t('loading.slow')}</p>}
      </div>
      {shown && slow && onRetry && <Button onClick={retry}>{t('common.retry')}</Button>}
    </div>
  );
}
