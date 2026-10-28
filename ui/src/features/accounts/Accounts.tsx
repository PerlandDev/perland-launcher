import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Avatar } from '../../art/Avatar';
import type { Account, AccountsFile } from '../../ipc/schema';
import { useAccountMutations, useAccounts, usePlaytime } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Check, Trash } from '../../ui/Icon';
import { formatPlaytime } from '../../ui/playtime';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { Medals } from '../medals/Medals';
import { MicrosoftSignIn } from './MicrosoftSignIn';
import { PerlandAccountCard } from './PerlandAccountCard';

export function Accounts() {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const playtime = usePlaytime();
  // Every instance's time on this computer, whoever played it: the launcher doesn't count per account.
  const totalPlayed = formatPlaytime(playtime.data?.reduce((sum, p) => sum + p.seconds, 0) ?? 0, t);
  // A failed switch is a toast; removing shows its error where the player is looking.
  const { setActive } = useAccountMutations();
  const { remove } = useAccountMutations({ inline: true });
  const [removing, setRemoving] = useState<Account | null>(null);
  // Who the player just switched to, for screen readers: the Use button they pressed is gone by then.
  const [switched, setSwitched] = useState<string | null>(null);
  const addHeading = useRef<HTMLHeadingElement>(null);
  const accountList = useRef<HTMLUListElement>(null);
  // The account whose Remove button should take focus once the row that was removed is gone.
  const focusAfterRemove = useRef<{ uuid: string | null } | null>(null);
  const switchOnce = useSingleFlight();
  const removeOnce = useSingleFlight();

  // Once the dialog is gone and the list is redrawn without the removed row, focus its stand-in.
  useEffect(() => {
    const target = focusAfterRemove.current;
    if (!target || removing) return;
    const row = target.uuid && accountList.current?.querySelector<HTMLElement>(`[data-remove-for="${target.uuid}"]`);
    (row || addHeading.current)?.focus();
    focusAfterRemove.current = null;
  }, [removing, accounts.data]);

  const announceActive = (file: AccountsFile) =>
    setSwitched(file.accounts.find((a) => a.uuid === file.active)?.username ?? null);

  const use = (account: Account) =>
    switchOnce((done) => setActive.mutate(account.uuid, { onSuccess: () => setSwitched(account.username), onSettled: done }));

  const closeRemove = () => {
    remove.reset();
    setRemoving(null);
  };
  const list = accounts.data?.accounts ?? [];
  const confirmRemove = (account: Account) =>
    removeOnce((done) =>
      remove.mutate(account.uuid, {
        onSuccess: (file) => {
          // The row is about to go: focus moves to the one that takes its place, or the one before it at the end.
          const at = list.findIndex((a) => a.uuid === account.uuid);
          const left = file.accounts;
          focusAfterRemove.current = { uuid: left[Math.min(at, left.length - 1)]?.uuid ?? null };
          if (account.uuid === accounts.data?.active) announceActive(file);
          closeRemove();
        },
        onSettled: done,
      }),
    );
  // Mirrors the core: removing the account in use promotes the first one left.
  const successor = removing && removing.uuid === accounts.data?.active ? list.find((a) => a.uuid !== removing.uuid) : undefined;
  const losesActive = removing !== null && removing.uuid === accounts.data?.active;
  return (
    <ScreenFrame title={t('accounts.title')} onBack={() => navigate({ name: 'home', id: null })}>
      <div className="grid max-w-2xl gap-10">
        <PerlandAccountCard />
        <div className="grid gap-2">
          <p role="status" className="sr-only">
            {switched ? t('accounts.nowUsing', { name: switched }) : ''}
          </p>
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
          <ul ref={accountList} className="grid gap-2">
            {list.map((account) => {
              const inUse = account.uuid === accounts.data?.active;
              return (
                <li key={account.uuid} className="panel flex items-center gap-4 px-4 py-3">
                  <Avatar uuid={account.uuid} size={40} />
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-bold">{account.username}</p>
                  </div>
                  {inUse ? (
                    <span className="flex items-center gap-2 font-semibold text-lime">
                      <Check className="h-5 w-5" aria-hidden />
                      {t('accounts.inUse')}
                    </span>
                  ) : (
                    <Button onClick={() => use(account)}>{t('accounts.use')}</Button>
                  )}
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={t('accounts.removeLabel', { name: account.username })}
                    icon={<Trash />}
                    data-remove-for={account.uuid}
                    onClick={() => setRemoving(account)}
                  />
                </li>
              );
            })}
          </ul>
          {totalPlayed && <p className="mt-1 text-sm text-ink-dim">{t('accounts.totalPlayed', { time: totalPlayed })}</p>}
        </div>

        <Medals />

        <section className="grid gap-4" aria-labelledby="add-heading">
          <h2 id="add-heading" ref={addHeading} tabIndex={-1} className="text-lg font-bold outline-none">
            {t('accounts.addTitle')}
          </h2>
          <MicrosoftSignIn variant="inline" />
        </section>
      </div>
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
          {t('accounts.removeBodyMicrosoft')}
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
    </ScreenFrame>
  );
}
