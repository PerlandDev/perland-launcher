// The Wearing row under the stage on the Create tab: Surprise me (a random outfit), then one chip per worn piece,
// each with × to take it off.
import { useTranslation } from 'react-i18next';
import type { CatalogPiece, WornPiece } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { Close } from '../../ui/Icon';
import { Dice } from './DiceIcon';
import type { Lang } from './catalogSearch';

/**
 * `pieces` is the catalog's (null while it isn't loaded or can't be reached: the names aren't known then, and taking a
 * piece off would redraw the skin without any of them, so the chips wait). A worn piece the loaded catalog lacks is
 * labelled unavailable and can still be taken off. `onSurprise` dresses the skin in a random outfit; absent when there
 * is no catalog to dress it from.
 */
export function WearingRow({
  worn,
  pieces,
  loading,
  lang,
  onTakeOff,
  onSurprise,
}: {
  worn: readonly WornPiece[];
  pieces: readonly CatalogPiece[] | null;
  loading: boolean;
  lang: Lang;
  onTakeOff: (id: string) => void;
  onSurprise?: () => void;
}) {
  const { t } = useTranslation();
  const byId = new Map((pieces ?? []).map((p) => [p.id, p]));
  return (
    <div className="wearing-row">
      {onSurprise && (
        <span className="wearing-surprise">
          <Button icon={<Dice aria-hidden />} title={t('skins.create.surpriseNote')} onClick={onSurprise}>
            {t('skins.create.surprise')}
          </Button>
        </span>
      )}
      <span className="wearing-label" aria-hidden>
        {t('skins.create.wearing')}
      </span>
      <ul className="wearing-chips" aria-label={t('skins.create.wearing')}>
        {worn.map((w) => {
          const piece = byId.get(w.id);
          const known = pieces !== null;
          const name = piece ? piece.name[lang] : known ? t('skins.create.unavailable') : loading ? t('skins.create.pieceLoading') : t('skins.create.pieceOffline');
          const note = piece ? undefined : known ? t('skins.create.unavailableNote') : loading ? undefined : t('skins.create.pieceOfflineNote');
          return (
            <li key={w.id} className="wearing-chip" data-gone={known && !piece ? true : undefined} data-waiting={known ? undefined : true} title={note}>
              <span className="truncate">{name}</span>
              <button
                type="button"
                className="wearing-x"
                disabled={!known}
                aria-label={t('skins.create.takeOffNamed', { name })}
                title={t('skins.create.takeOffNamed', { name })}
                onClick={() => onTakeOff(w.id)}
              >
                <Close aria-hidden />
              </button>
            </li>
          );
        })}
      </ul>
      {worn.length === 0 && <span className="text-sm text-ink-dim">{t('skins.create.wearingNone')}</span>}
    </div>
  );
}
