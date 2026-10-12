import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { ChevronDown } from './Icon';

/** The frame every non-home screen uses: a title row with optional back and actions, then content. */
export function ScreenFrame({
  title,
  subtitle,
  onBack,
  actions,
  wide,
  backLabel,
  children,
}: {
  title: string;
  subtitle?: string;
  onBack?: () => void;
  actions?: ReactNode;
  /** A 64rem column instead of 48rem, for screens with a grid or a side panel. */
  wide?: boolean;
  /** The back button's accessible name; "Back" by default. */
  backLabel?: string;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <section className="flex h-full flex-col">
      {/* The header spans the same 48rem column as the content (plus its padding), so actions line up with it. */}
      <header className={`flex ${wide ? 'max-w-[69rem]' : 'max-w-[53rem]'} flex-wrap items-end justify-between gap-6 px-10 pt-9 pb-6`}>
        <div className="flex min-w-0 items-center gap-4">
          {onBack && (
            <Button variant="ghost" size="icon" aria-label={backLabel ?? t('common.back')} onClick={onBack} icon={<ChevronDown className="rotate-90" />} />
          )}
          <div className="min-w-0">
            <h1 className="truncate font-display text-[2rem] leading-none">{title}</h1>
            {subtitle && <p className="mt-2 text-ink-dim">{subtitle}</p>}
          </div>
        </div>
        {actions && <div className="flex flex-wrap gap-3">{actions}</div>}
      </header>
      {/* Bottom padding lives inside the scroller's content, so a sticky action row meets the scroller's edge. */}
      <div className="min-h-0 flex-1 overflow-auto px-10">
        <div className={`pb-10 ${wide ? 'max-w-[64rem]' : ''}`}>{children}</div>
      </div>
    </section>
  );
}
