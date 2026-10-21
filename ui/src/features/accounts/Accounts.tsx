import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Avatar } from '../../art/Avatar';
import type { Account, AccountsFile } from '../../ipc/schema';
import { useAccountMutations, useAccounts, useAppInfo, usePlaytime } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ComingSoon } from '../../ui/ComingSoon';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Check, Trash, User } from '../../ui/Icon';
import { formatPlaytime } from '../../ui/playtime';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { OFFLINE_NAME } from './offlineName';
import { PerlandAccountCard } from './PerlandAccountCard';

export function Accounts() {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const appInfo = useAppInfo();
  const playtime = usePlaytime();
  // Every instance's time on this computer, whoever played it: the launcher doesn't count per account.
  const totalPlayed = formatPlaytime(playtime.data?.reduce((sum, p) => sum + p.seconds, 0) ?? 0, t);
  // A failed switch is a toast; adding and removing show their error where the player is looking.
  const { setActive } = useAccountMutations();
  const { addOffline: add, remove } = useAccountMutations({ inline: true });
  const [name, setName] = useState('');
  // The rule is only held against a name once the player has tried to add it or left the field with something in it.
  const [checked, setChecked] = useState(false);
  const [removing, setRemoving] = useState<Account | null>(null);
  // Who the player just switched to, for screen readers: the Use button they pressed is gone by then.
  const [switched, setSwitched] = useState<string | null>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const addHeading = useRef<HTMLHeadingElement>(null);
  const accountList = useRef<HTMLUListElement>(null);
  // The account whose Remove button should take focus once the row that was removed is gone.
  const focusAfterRemove = useRef<{ uuid: string | null } | null>(null);
  const switchOnce = useSingleFlight();
  const addOnce = useSingleFlight();
  const removeOnce = useSingleFlight();

  // Once the dialog is gone and the list is redrawn without the removed row, focus its stand-in.
  useEffect(() => {
    const target = focusAfterRemove.current;
    if (!target || removing) return;
    const row = target.uuid && accountList.current?.querySelector<HTMLElement>(`[data-remove-for="${target.uuid}"]`);
    (row || nameInput.current || addHeading.current)?.focus();
    focusAfterRemove.current = null;
  }, [removing, accounts.data]);

  const showInvalid = checked && !OFFLINE_NAME.test(name);

  const announceActive = (file: AccountsFile) =>
    setSwitched(file.accounts.find((a) => a.uuid === file.active)?.username ?? null);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!OFFLINE_NAME.test(name)) {
      setChecked(true);
      nameInput.current?.focus();
      return;
    }
    addOnce((done) =>
      add.mutate(name, {
        onSuccess: (file) => {
          setName('');
          setChecked(false);
          announceActive(file);
        },
        onSettled: done,
      }),
    );
  };

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
  const offlineLocked = appInfo.data?.offlineAllowed === false;
  const removeBody = !appInfo.data ? t('accounts.removeBodyPending') : offlineLocked ? t('accounts.removeBodyLocked') : t('accounts.removeBody');
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
                    <span className="badge mt-1">{t('accounts.offlineBadge')}</span>
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

        <section className="grid gap-4" aria-labelledby="add-heading">
          <h2 id="add-heading" ref={addHeading} tabIndex={-1} className="text-lg font-bold outline-none">
            {t('accounts.addOffline')}
          </h2>
          {offlineLocked ? (
            <p className="text-ink-dim">{t('accounts.offlineLocked')}</p>
          ) : (
            appInfo.data && (
              <form onSubmit={submit} className="flex items-start gap-3">
                <div className="flex-1">
                  <Field
                    label={t('accounts.offlineName')}
                    hint={t('accounts.offlineHint')}
                    error={showInvalid ? t('errors.offline_name_invalid.message') : null}
                  >
                    <input
                      ref={nameInput}
                      className="field-input"
                      value={name}
                      maxLength={16}
                      autoComplete="off"
                      onChange={(e) => setName(e.target.value)}
                      onBlur={() => name && setChecked(true)}
                    />
                  </Field>
                </div>
                <Button type="submit" variant="primary" className="mt-7" aria-disabled={add.isPending || undefined}>
                  {t('accounts.addOffline')}
                </Button>
              </form>
            )
          )}
          {add.isError && <ErrorCard error={add.error} />}
          <ComingSoon label={t('accounts.microsoft')} icon={<User className="h-5 w-5" />} />
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
          {removeBody}
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
