import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ChevronDown } from '../../ui/Icon';
import { TABS, type Tab } from './tabs';

/** The Skins screen's header: the back key, SKINS and the studio's tabs, then whatever the studio adds (the open
 *  skin's name, the save readout, History). It lives in the start bundle, not the studio's chunk, so the screen shows
 *  it the moment it opens, before three.js arrives. Without `onTab` the tabs wait for the studio: they show where it
 *  will open, but do nothing yet. */
export function SkinsHead({ tab, onTab, children }: { tab: Tab; onTab?: (tab: Tab) => void; children?: ReactNode }) {
  const { t } = useTranslation();
  return (
    <header className="skins-head">
      <Button variant="ghost" size="icon" aria-label={t('common.back')} title={t('common.back')} icon={<ChevronDown className="rotate-90" />} onClick={() => navigate({ name: 'home', id: null })} />
      <h1 id="skins-title" className="font-display text-[2rem] leading-none">
        {t('skins.title')}
      </h1>
      <div role="tablist" aria-labelledby="skins-title" className="skins-tabs">
        {TABS.map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            id={`skins-tab-${id}`}
            aria-selected={tab === id}
            aria-controls={onTab ? 'skins-tabpanel' : undefined}
            aria-disabled={onTab ? undefined : true}
            tabIndex={onTab ? undefined : -1}
            className="seg"
            data-checked={tab === id || undefined}
            onClick={() => onTab?.(id)}
          >
            {t(`skins.tab.${id}`)}
          </button>
        ))}
      </div>
      {children}
    </header>
  );
}
