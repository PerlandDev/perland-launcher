import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { isLauncherError } from '../../ipc/errors';
import { inviteCanonical, isInviteLink } from '../../ipc/inviteLink';
import type { LauncherError } from '../../ipc/schema';
import { useInstances, useJoinAdd, useJoinPreview, useServerRelink } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { copyText } from '../../ui/copyText';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Copy, WarningDiamond } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { InviteOnly } from '../servers/InviteOnly';
import { ServerCard } from './ServerCard';

const notAnInvite: LauncherError = {
  category: 'pack',
  code: 'invite_link_invalid',
  message: "That isn't a Perland invite link",
  fix: null,
  detail: null,
  params: {},
};

/** Confirm a server before adding it (or before moving an instance to a new link). Nothing is
 *  written until the button. */
export function JoinServer({ invite, relinkFor }: { invite: string; relinkFor?: string }) {
  const { t } = useTranslation();
  const listing = useInstances();
  const [draft, setDraft] = useState(invite);
  const [link, setLink] = useState(invite.trim());
  const valid = isInviteLink(link);
  // The canonical spelling keys the cache, so every spelling of one link shares an entry.
  const preview = useJoinPreview(valid ? (inviteCanonical(link) ?? link) : '');
  const add = useJoinAdd({ inline: true });
  const relink = useServerRelink(relinkFor ?? '', { inline: true });
  const target = relinkFor ? (listing.data?.instances.find((i) => i.id === relinkFor) ?? null) : null;
  const asking = relinkFor !== undefined || !valid;
  const data = preview.data;
  const oldName = target?.server?.name ?? target?.name ?? '';
  const mismatch = data && target && data.name.trim().toLowerCase() !== oldName.trim().toLowerCase();
  const address = isLauncherError(preview.error) && preview.error.code === 'invite_unsupported' ? preview.error.params.address : undefined;
  const busy = add.isPending || relink.isPending;

  const check = (event: FormEvent) => {
    event.preventDefault();
    add.reset();
    relink.reset();
    setLink(draft.trim());
  };
  const go = () => {
    if (!data || busy) return;
    if (relinkFor) relink.mutate(link, { onSuccess: () => navigate({ name: 'home', id: relinkFor }) });
    else if (data.existing) navigate({ name: 'home', id: data.existing });
    else add.mutate(link, { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) });
  };
  const label = relinkFor ? t('join.useLink') : data?.existing ? t('join.open') : busy ? t('join.adding') : t('join.add');

  return (
    <ScreenFrame title={relinkFor ? t('join.relinkTitle') : t('join.title')}>
      <div className="grid max-w-3xl gap-6">
        {asking && (
          <form onSubmit={check} noValidate className="flex flex-wrap items-end gap-3">
            <Field label={t('join.linkLabel')}>
              <input
                className="field-input min-w-[28rem]"
                type="url"
                inputMode="url"
                spellCheck={false}
                value={draft}
                placeholder={t('join.linkPlaceholder')}
                onChange={(e) => setDraft(e.target.value)}
              />
            </Field>
            {/* Until a server is on screen, checking the link is the act; then the screen's own button is. */}
            <Button type="submit" variant={data ? 'secondary' : 'primary'} disabled={!draft.trim()}>
              {t('join.check')}
            </Button>
          </form>
        )}
        {link && !valid && <ErrorCard error={notAnInvite} />}
        {preview.isError && (
          <ErrorCard
            error={preview.error}
            actions={
              address ? (
                <Button icon={<Copy />} onClick={() => void copyText(address)}>
                  {t('join.copyAddress')}
                </Button>
              ) : undefined
            }
          />
        )}
        {data && <ServerCard preview={data} />}
        {data?.whitelisted && <InviteOnly />}
        {mismatch && (
          <p role="alert" className="flex items-start gap-2 font-semibold">
            <WarningDiamond className="mt-0.5 h-5 w-5 flex-none text-coral" aria-hidden />
            {t('join.mismatch', { new: data.name, old: oldName })}
          </p>
        )}
        {add.isError && <ErrorCard error={add.error} />}
        {relink.isError && <ErrorCard error={relink.error} />}
        {data && (
          <div className="sticky bottom-0 flex items-center justify-end gap-4 bg-ground py-4 shadow-[0_-1px_0_var(--color-line)]">
            {!relinkFor && !data.existing && <p className="text-sm text-ink-dim">{t('join.note')}</p>}
            <Button onClick={() => navigate({ name: 'home', id: relinkFor ?? null })}>{t('common.cancel')}</Button>
            <Button variant="primary" size="lg" disabled={busy} onClick={go}>
              {label}
            </Button>
          </div>
        )}
      </div>
    </ScreenFrame>
  );
}
