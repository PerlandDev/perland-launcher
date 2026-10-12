import { useTranslation } from 'react-i18next';
import { navigate, type ModsTab } from '../../state/route';

const MODS_TABS: ModsTab[] = ['mod', 'resource_pack', 'shader'];

/** Mods · Resource packs · Shaders. Each tab is a route of its own, so Back from Browse can return to it. */
export function ContentTabs({ id, active }: { id: string; active: ModsTab }) {
  const { t } = useTranslation();
  return (
    <div role="tablist" aria-label={t('mods.screenTitle')} className="flex flex-wrap gap-2">
      {MODS_TABS.map((kind) => (
        <button
          key={kind}
          type="button"
          role="tab"
          className="seg"
          aria-selected={kind === active}
          data-checked={kind === active ? '' : undefined}
          onClick={() => navigate({ name: 'mods', id, tab: kind === 'mod' ? undefined : kind })}
        >
          {t(`mods.tabs.${kind}`)}
        </button>
      ))}
    </div>
  );
}
