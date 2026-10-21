import { useTranslation } from 'react-i18next';
import type { FriendsCheck } from '../../ipc/schema';

/** What a server made from an instance's server pack gets, as Share's file rows without toggles: Mods (the count over
 *  `memoryLine`) and Settings, then what stays on the computer. */
export function ServerPackRows({ data, memoryLine }: { data: FriendsCheck; memoryLine: string }) {
  const { t } = useTranslation();
  return (
    <section aria-labelledby="friends-what" className="grid gap-1">
      <h3 id="friends-what" className="text-sm font-semibold text-ink">
        {t('friends.whatGoes')}
      </h3>
      <ul>
        <li className="content-row">
          <div className="min-w-0 flex-1">
            <p className="font-semibold text-ink">{t('friends.mods')}</p>
            <p className="tabular text-sm text-ink-dim">{t('friends.modsCount', { count: data.mods })}</p>
            <p className="tabular text-sm text-ink">{memoryLine}</p>
          </div>
        </li>
        <li className="content-row">
          <div className="min-w-0 flex-1">
            <p className="font-semibold text-ink">{t('friends.settings')}</p>
            <p className="tabular text-sm text-ink-dim">{t('friends.settingsCount', { count: data.settings })}</p>
          </div>
        </li>
      </ul>
      <p className="text-sm text-ink-dim">{t('friends.stays')}</p>
    </section>
  );
}
