import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Shirt } from '../../ui/Icon';
import { PlayerFigure } from '../player/PlayerFigure';

/** Home's player: the active account's figure standing on Play's own top edge (who is about to play), with a raised
 *  Change skin key and the keys that sat above Play (`children`: sign in, Continue) beside it. The figure waves while the
 *  pointer or the keyboard is on the key or on it, and holds still while a game runs (`still`). Nothing here is lit.
 *  Not `shown` (hidden in Settings, no account, no room), only the keys remain, in the same place in the tree, so
 *  showing or hiding the figure never remounts them. */
export function PlayerStand({ shown, still, children }: { shown: boolean; still: boolean; children: ReactNode }) {
  const { t } = useTranslation();
  const [wave, setWave] = useState(false);
  const open = () => navigate({ name: 'skins' });
  const hover = { onPointerEnter: () => setWave(true), onPointerLeave: () => setWave(false) };
  return (
    <div className="stand" data-player={shown ? '' : undefined}>
      {/* A picture of the key beside it: a click does the same, for the mouse only; the key carries the name. */}
      {shown && (
        <button type="button" className="stand-player" tabIndex={-1} aria-hidden onClick={open} {...hover}>
          <PlayerFigure wave={wave} still={still} />
        </button>
      )}
      <div className="stand-keys">
        {shown && (
          <Button icon={<Shirt aria-hidden />} onClick={open} onFocus={() => setWave(true)} onBlur={() => setWave(false)} {...hover}>
            {t('home.changeSkin')}
          </Button>
        )}
        {children}
      </div>
    </div>
  );
}
