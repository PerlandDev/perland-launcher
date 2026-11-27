import { useQueryClient } from '@tanstack/react-query';
import { useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type { AccountsFile } from '../../ipc/schema';
import { qk, useAccountMutations, useAppInfo } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { MicrosoftSignIn } from '../accounts/MicrosoftSignIn';
import { OFFLINE_NAME } from '../accounts/offlineName';

/** Add an account, in the standard dialog: the Microsoft sign-in (its one lit key), then, where offline accounts are
 *  allowed, a player name for an offline one. `onAdded` hears the accounts as they are once one was added; the dialog's
 *  owner closes it. A failed add is said here, under the form, and the name stays. */
export function AddAccount({ open, onClose, onAdded }: { open: boolean; onClose: () => void; onAdded: (file: AccountsFile) => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const offlineAllowed = useAppInfo().data?.offlineAllowed === true;
  const { addOffline: add } = useAccountMutations({ inline: true });
  const [name, setName] = useState('');
  // The rule is only held against a name once the player has tried to add it or left the field with something in it.
  const [checked, setChecked] = useState(false);
  const nameInput = useRef<HTMLInputElement>(null);
  const addOnce = useSingleFlight();
  const showInvalid = checked && !OFFLINE_NAME.test(name);

  const close = () => {
    add.reset();
    setName('');
    setChecked(false);
    onClose();
  };

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
          onAdded(file);
        },
        onSettled: done,
      }),
    );
  };

  const signedIn = () => {
    const file = qc.getQueryData<AccountsFile>(qk.accounts);
    if (file) onAdded(file);
  };

  return (
    <Dialog
      open={open}
      onClose={close}
      title={t('accounts.addTitle')}
      actions={
        <Button variant="ghost" onClick={close}>
          {t('common.close')}
        </Button>
      }
    >
      <div className="mt-4 grid gap-6 text-ink">
        <MicrosoftSignIn variant="inline" onSignedIn={signedIn} />
        {offlineAllowed && (
          <section aria-labelledby="add-offline" className="grid gap-4">
            <div className="flex items-center gap-3">
              <h3 id="add-offline" className="text-sm font-bold text-ink-dim">
                {t('firstRun.offlineTitle')}
              </h3>
              <span aria-hidden className="h-px flex-1 bg-line" />
            </div>
            <form onSubmit={submit} className="grid justify-items-start gap-4">
              <div className="w-full">
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
              {/* Microsoft's is the dialog's one lit key, so this one stays raised. */}
              <Button type="submit" aria-disabled={add.isPending || undefined}>
                {t('accounts.addOffline')}
              </Button>
            </form>
          </section>
        )}
        {add.isError && <ErrorCard error={add.error} />}
      </div>
    </Dialog>
  );
}
