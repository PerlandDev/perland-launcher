import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Account, AccountsFile } from '../../ipc/schema';
import { useAccountMutations, useAccounts, useAppInfo } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Check, Plus, Shirt, Trash } from '../../ui/Icon';
import { Menu } from '../../ui/Menu';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { AccountAvatar } from '../accounts/AccountAvatar';
import { AddAccount } from './AddAccount';
import { PerlandRow } from './PerlandRow';
import { useFocusOn } from './useFocusOn';

/** The key that opens a row's menu, and the panel's own Add key: where the keyboard goes when a row is gone. */
const menuKey = (row: string) => `.profile [data-row="${row}"] [aria-haspopup="menu"]`;
const ADD_KEY = '.profile [data-add-account]';

/** The Profile's Accounts panel: the Perland account first (once signed in), then every Minecraft account as a row (its
 *  face, its name over its kind in words, the lime In use or a raised Use, and a menu with Edit skin and Remove), then Add
 *  an account as the list's last row, which opens its dialog. `say` speaks a change to screen readers; `add` opens the dialog on arrival. */
export function AccountsPanel({ say, add = false }: { say: (text: string) => void; add?: boolean }) {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const appInfo = useAppInfo();
  // A failed switch is a toast; removing shows its error where the player is looking.
  const { setActive } = useAccountMutations();
  const { remove } = useAccountMutations({ inline: true });
  const [removing, setRemoving] = useState<Account | null>(null);
  const [adding, setAdding] = useState(add);
  const switchOnce = useSingleFlight();
  const removeOnce = useSingleFlight();
  const focusOn = useFocusOn(removing !== null || adding);

  const list = accounts.data?.accounts ?? [];
  const sayActive = (file: AccountsFile) => {
    const name = file.accounts.find((a) => a.uuid === file.active)?.username;
    say(name ? t('accounts.nowUsing', { name }) : '');
  };

  const use = (account: Account) =>
    switchOnce((done) =>
      setActive.mutate(account.uuid, {
        onSuccess: () => {
          say(t('accounts.nowUsing', { name: account.username }));
          // The Use key is gone from the row now: its menu takes the keyboard.
          focusOn(menuKey(account.uuid));
        },
        onSettled: done,
      }),
    );

  const closeRemove = () => {
    remove.reset();
    setRemoving(null);
  };
  const confirmRemove = (account: Account) =>
    removeOnce((done) =>
      remove.mutate(account.uuid, {
        onSuccess: (file) => {
          // The row is about to go: focus moves to the one that takes its place, or the one before it at the end.
          const at = list.findIndex((a) => a.uuid === account.uuid);
          const left = file.accounts;
          const next = left[Math.min(at, left.length - 1)]?.uuid;
          focusOn(...(next ? [menuKey(next)] : []), ADD_KEY);
          if (account.uuid === accounts.data?.active) sayActive(file);
          closeRemove();
        },
        onSettled: done,
      }),
    );

  const added = (file: AccountsFile) => {
    // The dialog hands the keyboard back to the Add key it opened from.
    setAdding(false);
    sayActive(file);
  };

  const offlineAllowed = appInfo.data?.offlineAllowed === true;
  /** A Microsoft account comes back with a sign-in; an offline one only where offline accounts can still be added. */
  const removeBody = (account: Account) =>
    account.provider === 'microsoft'
      ? t('accounts.removeBodyMicrosoft')
      : offlineAllowed
        ? t('accounts.removeBody')
        : t('accounts.removeBodyPending');
  // Mirrors the core: removing the account in use promotes the first one left.
  const successor = removing && removing.uuid === accounts.data?.active ? list.find((a) => a.uuid !== removing.uuid) : undefined;
  const losesActive = removing !== null && removing.uuid === accounts.data?.active;

  return (
    <section className="panel profile-card profile-accounts-card" aria-labelledby="profile-accounts">
      <h2 id="profile-accounts" className="profile-title">
        {t('accounts.title')}
      </h2>
      {accounts.isPending && <p className="text-ink-dim">{t('common.loading')}</p>}
      {accounts.isError && (
        <ErrorCard
          error={accounts.error}
          actions={
            <Button disabled={accounts.isFetching} onClick={() => void accounts.refetch()}>
              {t('common.retry')}
            </Button>
          }
        />
      )}
      {accounts.isSuccess && list.length === 0 && <p className="text-ink-dim">{t('accounts.empty')}</p>}
      <ul className="profile-accounts">
        <PerlandRow
          onSignedOut={() => {
            say(t('perland.signedOut'));
            focusOn('.profile [data-step="perland"] button', ADD_KEY);
          }}
        />
        {list.map((account) => {
          const inUse = account.uuid === accounts.data?.active;
          return (
            <li key={account.uuid} className="profile-account" data-row={account.uuid} data-in-use={inUse ? '' : undefined}>
              <AccountAvatar account={account} size={32} />
              <div className="profile-who">
                <p className="truncate font-bold" title={account.username}>
                  {account.username}
                </p>
                {/* Every row says its kind in words: a face alone can't tell them apart (#157). */}
                <p className="truncate text-sm text-ink-dim">
                  {account.provider === 'microsoft' ? t('accounts.kindMicrosoft') : t('accounts.kindOffline')}
                </p>
              </div>
              {inUse ? (
                <span className="profile-state profile-in-use">
                  <Check aria-hidden />
                  {t('accounts.inUse')}
                </span>
              ) : (
                <Button className="profile-state profile-use" onClick={() => use(account)}>
                  {t('accounts.use')}
                </Button>
              )}
              <Menu
                compact
                floating
                className="relative profile-menu"
                label={t('profile.more', { name: account.username })}
                items={[
                  {
                    id: 'skin',
                    label: t('accounts.editSkin'),
                    icon: <Shirt aria-hidden />,
                    onSelect: () => navigate({ name: 'skins', account: account.uuid }),
                  },
                  { id: 'remove', label: t('accounts.remove'), icon: <Trash aria-hidden />, danger: true, onSelect: () => setRemoving(account) },
                ]}
              />
            </li>
          );
        })}
      </ul>
      {/* The list's last row, on the rows' own grid: an empty socket where a face goes, then the words. */}
      <button type="button" className="profile-add" data-add-account onClick={() => setAdding(true)}>
        <span className="profile-add-slot" aria-hidden>
          <Plus />
        </span>
        {t('accounts.addTitle')}
      </button>
      <AddAccount open={adding} onClose={() => setAdding(false)} onAdded={added} />
      {removing && (
        <Dialog
          open
          onClose={closeRemove}
          title={t('accounts.removeTitle', { name: removing.username })}
          actions={
            <>
              <Button onClick={closeRemove}>{t('common.cancel')}</Button>
              <Button variant="danger" aria-disabled={remove.isPending || undefined} onClick={() => confirmRemove(removing)}>
                {t('accounts.remove')}
              </Button>
            </>
          }
        >
          {removeBody(removing)}
          {losesActive && (
            <p className="mt-2">{successor ? t('accounts.removeNext', { name: successor.username }) : t('accounts.removeNone')}</p>
          )}
          {remove.isError && (
            <div className="mt-4">
              <ErrorCard error={remove.error} />
            </div>
          )}
        </Dialog>
      )}
    </section>
  );
}
