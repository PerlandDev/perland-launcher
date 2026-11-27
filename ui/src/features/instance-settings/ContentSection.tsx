import { useTranslation } from 'react-i18next';
import type { Instance } from '../../ipc/schema';
import { navigate, type ModsTab } from '../../state/route';
import { Button } from '../../ui/Button';
import { useContentCounts } from '../mods/counts';
import { TAB_GLYPHS } from '../mods/tabGlyphs';

const TABS: ModsTab[] = ['mod', 'resource_pack', 'shader'];

/** Instance settings' way into Mods & packs: a key per tab with what it holds, then where more comes from. */
export function ContentSection({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const counts = useContentCounts(instance.id, instance);
  return (
    <section className="grid gap-3" aria-labelledby="content-heading">
      <h2 id="content-heading" tabIndex={-1} className="scroll-mt-6 justify-self-start text-lg font-bold">
        {t('mods.screenTitle')}
      </h2>
      <div className="flex flex-wrap gap-3">
        {TABS.map((tab) => {
          const count = counts[tab];
          const Glyph = TAB_GLYPHS[tab];
          return (
            <Button key={tab} icon={<Glyph aria-hidden />} onClick={() => navigate({ name: 'mods', id: instance.id, tab: tab === 'mod' ? undefined : tab })}>
              {t(`mods.tabs.${tab}`)}
              {/* Only once known: a 0 while it loads would say the folder is empty. */}
              {count !== undefined && (
                <>
                  {' '}
                  <span className="tag tabular">{count}</span>
                </>
              )}
            </Button>
          );
        })}
      </div>
      <p className="text-sm text-ink-dim">
        {instance.game.loader ? t('instanceSettings.contentMoreModded') : t('instanceSettings.contentVanilla')}
      </p>
    </section>
  );
}
