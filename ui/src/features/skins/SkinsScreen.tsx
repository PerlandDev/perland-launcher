import { lazy, Suspense, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { ScreenBoundary } from '../../ui/ScreenBoundary';
import { screenError } from '../../ui/screenError';
import { ScreenLoading } from '../../ui/ScreenLoading';
import { loadSkinStudio } from './loadSkinStudio';
import { SkinsHead } from './SkinsHead';
import { FIRST_TAB } from './tabs';

const lazyStudio = () => lazy(loadSkinStudio);
/** Kept between visits, so a studio that already arrived opens at once. Only Try again replaces it: `lazy` holds on
 *  to a failed load for good, so a new one is what loads the chunk again. */
let studio = lazyStudio();
const freshStudio = () => (studio = lazyStudio());

/** The screen around the studio while it isn't there: the real header (its tabs waiting), and what stands in the body. */
function Waiting({ children }: { children: ReactNode }) {
  return (
    <section className="skins-screen" aria-labelledby="skins-title">
      <SkinsHead tab={FIRST_TAB} />
      <div className="skins-waiting">{children}</div>
    </section>
  );
}

/** The Skins screen. The studio (three.js) loads on first visit, or earlier (see `prefetchSkinStudio`); until it
 *  arrives the header is already up with a loading line, and a studio that fails to load or draw shows an error card
 *  with Try again instead of an empty window. */
export function SkinsScreen(props: { skin?: string; account?: string }) {
  const { t } = useTranslation();
  const [Studio, setStudio] = useState(() => studio);
  const loadAgain = () => {
    const fresh = freshStudio();
    setStudio(() => fresh);
  };
  return (
    <ScreenBoundary
      fallback={(error, reset) => (
        <Waiting>
          <div className="w-full max-w-xl">
            <ErrorCard
              error={screenError('skins_open_failed', error)}
              actions={
                <Button
                  onClick={() => {
                    loadAgain();
                    reset();
                  }}
                >
                  {t('common.retry')}
                </Button>
              }
            />
          </div>
        </Waiting>
      )}
    >
      <Suspense
        fallback={
          <Waiting>
            <ScreenLoading label={t('loading.skins')} place="center" onRetry={loadAgain} />
          </Waiting>
        }
      >
        <Studio {...props} />
      </Suspense>
    </ScreenBoundary>
  );
}
