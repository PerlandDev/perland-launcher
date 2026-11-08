// The Wearing row under the stage on the Create tab: one chip per worn piece, each with × to take it off.
import { useTranslation } from 'react-i18next';
import type { CatalogPiece, WornPiece } from '../../ipc/schema';
import { Close } from '../../ui/Icon';
import type { Lang } from './catalogSearch';

export function WearingRow({ worn, pieces, lang, onTakeOff }: { worn: readonly WornPiece[]; pieces: readonly CatalogPiece[]; lang: Lang; onTakeOff: (id: string) => void }) {
  const { t } = useTranslation();
  const byId = new Map(pieces.map((p) => [p.id, p]));
  return (
    <div className="wearing-row">
      <span className="wearing-label" aria-hidden>
        {t('skins.create.wearing')}
      </span>
      <ul className="wearing-chips" aria-label={t('skins.create.wearing')}>
        {worn.map((w) => {
          const piece = byId.get(w.id);
          const name = piece ? piece.name[lang] : t('skins.create.unavailable');
          return (
            <li key={w.id} className="wearing-chip" data-gone={piece ? undefined : true} title={piece ? undefined : t('skins.create.unavailableNote')}>
              <span className="truncate">{name}</span>
              <button type="button" className="wearing-x" aria-label={t('skins.create.takeOffNamed', { name })} title={t('skins.create.takeOffNamed', { name })} onClick={() => onTakeOff(w.id)}>
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
