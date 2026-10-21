import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { navigate } from '../../state/route';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { ManageConsole } from './ManageConsole';
import { ManageOverview } from './ManageOverview';
import { ManagePlayers } from './ManagePlayers';
import { ManageSettings } from './ManageSettings';

const TABS = ['players', 'console', 'settings'] as const;
type Tab = (typeof TABS)[number];

/** Manage: running a server the player owns or helps run, from the launcher. The overview (state, the one big button, the
 *  address) stays on top; Players, Console and Settings are the pressed-segment tabs under it, one at a time, so the console
 *  follows the log only while it is on screen. Back goes to the servers, where Manage is opened from. */
export function ManageScreen({ tenantId, stack }: { tenantId: string; stack: string }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>('players');
  return (
    <ScreenFrame title={stack} onBack={() => navigate({ name: 'servers' })}>
      <div className="grid max-w-[48rem] gap-8">
        <ManageOverview tenantId={tenantId} stack={stack} />
        <fieldset className="flex flex-wrap gap-2">
          <legend className="sr-only">{t('manage.sections')}</legend>
          {TABS.map((name) => (
            <label key={name} className="seg" data-checked={tab === name ? '' : undefined}>
              <input type="radio" name="manage-tab" className="sr-only" checked={tab === name} onChange={() => setTab(name)} />
              {t(`manage.tabs.${name}`)}
            </label>
          ))}
        </fieldset>
        {tab === 'players' && <ManagePlayers tenantId={tenantId} stack={stack} onOpenSettings={() => setTab('settings')} />}
        {tab === 'console' && <ManageConsole tenantId={tenantId} stack={stack} />}
        {tab === 'settings' && <ManageSettings tenantId={tenantId} stack={stack} />}
      </div>
    </ScreenFrame>
  );
}
