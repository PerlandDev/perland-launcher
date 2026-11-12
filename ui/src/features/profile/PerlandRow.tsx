import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import mark from '../../assets/mark.svg';
import { commands } from '../../ipc/commands';
import { describeError } from '../../ipc/errors';
import { usePerlandStatus } from '../../state/queries';
import { Logout, WarningDiamond } from '../../ui/Icon';
import { Menu } from '../../ui/Menu';
import { perlandName, setPerlandAccount } from '../accounts/perlandSession';

/** The signed-in Perland account, as the first row of Accounts: the land-plot mark, the account's name and under it its
 *  email when that is not already the name, else how it signs in. Its menu signs out. It renders nothing signed out (the
 *  account is then a step of Finish setting up) or while the Engine has player accounts closed. */
export function PerlandRow({ onSignedOut }: { onSignedOut?: () => void }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const status = usePerlandStatus();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const account = status.data?.enabled ? status.data.account : null;
  if (!account) return null;

  const name = perlandName(account, t);
  // The Engine names the account by Discord first, then GitHub, so the method follows the same order.
  const method = account.discord ? t('perland.viaDiscord') : account.github ? t('perland.viaGithub') : null;
  const second = account.email && account.email !== name ? account.email : method;

  const signOut = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await commands.perlandSignOut();
      setPerlandAccount(qc, null);
      onSignedOut?.();
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  };

  const failure = error === null ? null : describeError(i18n, error);
  return (
    <li className="profile-account" data-row="perland">
      <span className="profile-mark" aria-hidden>
        <img src={mark} alt="" width={21} height={18} />
      </span>
      <div className="profile-who">
        <p className="truncate font-bold" title={name}>
          {name}
        </p>
        <p className="truncate text-sm text-ink-dim" title={second ?? undefined}>
          {t('perland.title')}
          {second ? ` · ${second}` : ''}
        </p>
      </div>
      <Menu
        compact
        floating
        label={t('profile.morePerland')}
        items={[{ id: 'sign-out', label: t('perland.signOut'), icon: <Logout aria-hidden />, onSelect: () => void signOut() }]}
      />
      {failure && (
        <div role="alert" className="basis-full grid gap-1 text-sm">
          <p className="flex items-start gap-2 font-semibold text-coral">
            <WarningDiamond className="mt-0.5 h-4 w-4 flex-none" aria-hidden />
            {failure.message}
          </p>
          {failure.fix && <p className="pl-6 text-ink-dim">{failure.fix}</p>}
        </div>
      )}
    </li>
  );
}
