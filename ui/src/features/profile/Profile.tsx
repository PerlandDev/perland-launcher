import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { AccountsFile } from '../../ipc/schema';
import { qk, useAccounts, usePlayerSkin, usePlaytime } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Shirt } from '../../ui/Icon';
import { formatPlaytime } from '../../ui/playtime';
import { Medals } from '../medals/Medals';
import { PlayerFigure } from '../player/PlayerFigure';
import { AccountsPanel } from './AccountsPanel';
import { SetUp } from './SetUp';
import { useFocusOn } from './useFocusOn';

/** The player's skin: the fitting room's recessed well with their own figure standing in it, how full the wardrobe and the
 *  catalog are, and the key to the Skins screen. */
function SkinCard() {
  const { t } = useTranslation();
  const skin = usePlayerSkin();
  // The catalog is the Skins screen's own query, so a visit there finds it warm.
  const catalog = useQuery({ queryKey: ['skinCatalog'], queryFn: commands.skinCatalog });
  const looks = catalog.data?.manifest?.looks.length ?? 0;
  const wardrobe = skin.data?.wardrobe;
  const [wave, setWave] = useState(false);
  const open = () => navigate({ name: 'skins' });
  return (
    <section className="panel profile-card" aria-labelledby="profile-skin">
      {/* The figure is the picture, as in the fitting room. A click on it is the key below, for the mouse only. */}
      <div className="well profile-stage" onClick={open} onPointerEnter={() => setWave(true)} onPointerLeave={() => setWave(false)}>
        <PlayerFigure wave={wave} />
      </div>
      <div className="grid gap-1">
        <h2 id="profile-skin" className="profile-title">
          {t('profile.skin')}
        </h2>
        <p className="text-sm text-ink-dim">
          {wardrobe !== undefined && (wardrobe > 0 ? t('profile.wardrobe', { count: wardrobe }) : t('profile.wardrobeEmpty'))}
          {looks > 0 && ` ${t('profile.looks', { count: looks })}`}
        </p>
      </div>
      <div>
        <Button icon={<Shirt aria-hidden />} onClick={open} onFocus={() => setWave(true)} onBlur={() => setWave(false)}>
          {t('home.changeSkin')}
        </Button>
      </div>
    </section>
  );
}

/** The Profile: everything about the player on one page, opened from the account in the title bar. Who is playing heads
 *  it; then, while something is still to connect, Finish setting up; beside the player's skin and their accounts, what they
 *  have earned (the medals, and the achievements on Perland servers). It takes the place of the Accounts and Achievements
 *  screens. `add` opens Add an account on arrival. */
export function Profile({ add = false }: { add?: boolean }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const accounts = useAccounts();
  const playtime = usePlaytime();
  const active = accounts.data?.accounts.find((a) => a.uuid === accounts.data?.active) ?? null;
  // Every instance's time on this computer, whoever played it: the launcher doesn't count per account.
  const played = formatPlaytime(playtime.data?.reduce((sum, p) => sum + p.seconds, 0) ?? 0, t);
  // One voice for the page's changes: who is in use now, who signed in. The control that made the change is often gone by then.
  const [said, say] = useState('');
  const focusOn = useFocusOn();
  const kind = active ? (active.provider === 'microsoft' ? t('accounts.kindMicrosoft') : t('accounts.kindOffline')) : null;
  const facts = [kind, played ? t('accounts.totalPlayed', { time: played }) : null].filter(Boolean);
  return (
    <section className="profile" aria-labelledby="profile-title">
      <header className="profile-head">
        <h1 id="profile-title" className="truncate font-display text-[2rem] leading-none">
          {active?.username ?? t('profile.title')}
        </h1>
        {facts.length > 0 && <p className="mt-2 text-ink-dim">{facts.join(' · ')}</p>}
      </header>
      <div className="profile-scroll">
        <p role="status" className="sr-only">
          {said}
        </p>
        <div className="profile-grid">
          <div className="profile-setup">
            <SetUp
              onMicrosoft={() => {
                const file = qc.getQueryData<AccountsFile>(qk.accounts);
                const name = file?.accounts.find((a) => a.uuid === file.active)?.username;
                if (name) say(t('accounts.microsoftSignedIn', { name }));
                focusOn('.profile [data-in-use] [aria-haspopup="menu"]');
              }}
              onPerland={(name) => {
                say(t('perland.signedIn', { name }));
                focusOn('.profile [data-row="perland"] [aria-haspopup="menu"]');
              }}
            />
          </div>
          <div className="profile-me">
            <SkinCard />
            <AccountsPanel say={say} add={add} />
          </div>
          <div className="profile-earned">
            <Medals />
          </div>
        </div>
      </div>
    </section>
  );
}
