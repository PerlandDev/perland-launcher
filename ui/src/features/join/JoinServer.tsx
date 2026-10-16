import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { isLauncherError } from '../../ipc/errors';
import { inviteCanonical, isInviteLink } from '../../ipc/inviteLink';
import type { Instance, JoinPreview, LauncherError } from '../../ipc/schema';
import { useInstances, useJoinAdd, useJoinPreview, useServerLink, useServerRelink } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { copyText } from '../../ui/copyText';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Copy, WarningDiamond } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { loaderName } from '../home/labels';
import { InviteOnly } from '../servers/InviteOnly';
import { savedServerName } from '../servers/serverName';
import { ServerCard } from './ServerCard';

const notAnInvite: LauncherError = {
  category: 'pack',
  code: 'invite_link_invalid',
  message: "That isn't a Perland invite link",
  fix: null,
  detail: null,
  params: {},
};

/** "Fabric" for the Engine's `fabric`; an unknown loader as the Engine spells it. */
function loaderLabel(kind: string): string {
  return kind === 'fabric' || kind === 'quilt' || kind === 'forge' || kind === 'neoforge' ? loaderName(kind) : kind;
}

/** The loader a server instance gets, as the core decides it: none without a modpack, and
 *  `vanilla` is none. */
function serverLoader(preview: JoinPreview): string | null {
  const loader = preview.pack ? (preview.game.loader?.trim().toLowerCase() ?? '') : '';
  return loader && loader !== 'vanilla' ? loader : null;
}

/** Whether linking would move `instance` to another Minecraft version or loader (a loader update
 *  alone keeps worlds working). */
function changesGame(instance: Instance, preview: JoinPreview): boolean {
  return (
    (preview.game.version !== null && preview.game.version !== instance.game.minecraft) ||
    serverLoader(preview) !== (instance.game.loader?.kind ?? null)
  );
}

/** Confirm a server before adding it (or before moving an instance to a new link, or linking a
 *  vanilla or imported instance to it). Nothing is written until the button. */
export function JoinServer({ invite, relinkFor, linkFor }: { invite: string; relinkFor?: string; linkFor?: string }) {
  const { t } = useTranslation();
  const listing = useInstances();
  const [draft, setDraft] = useState(invite);
  const [link, setLink] = useState(invite.trim());
  const valid = isInviteLink(link);
  // The canonical spelling keys the cache, so every spelling of one link shares an entry.
  const preview = useJoinPreview(valid ? (inviteCanonical(link) ?? link) : '');
  const add = useJoinAdd({ inline: true });
  const relink = useServerRelink(relinkFor ?? '', { inline: true });
  const linkTo = useServerLink(linkFor ?? '', { inline: true });
  const target = relinkFor ? (listing.data?.instances.find((i) => i.id === relinkFor) ?? null) : null;
  const linking = linkFor ? (listing.data?.instances.find((i) => i.id === linkFor) ?? null) : null;
  const asking = relinkFor !== undefined || linkFor !== undefined || !valid;
  const data = preview.data;
  const oldName = target?.server?.name ?? target?.name ?? '';
  // The saved name was cut to the core's limit; the preview's is whole. Compare them cut alike.
  const mismatch = data && target && savedServerName(data.name).trim().toLowerCase() !== oldName.trim().toLowerCase();
  const address = isLauncherError(preview.error) && preview.error.code === 'invite_unsupported' ? preview.error.params.address : undefined;
  const busy = add.isPending || relink.isPending || linkTo.isPending;
  const gameMove =
    data && linking && data.game.version && changesGame(linking, data) ? { version: data.game.version, loader: serverLoader(data) } : null;

  const check = (event: FormEvent) => {
    event.preventDefault();
    add.reset();
    relink.reset();
    linkTo.reset();
    setLink(draft.trim());
  };
  const go = () => {
    if (!data || busy) return;
    if (relinkFor) relink.mutate(link, { onSuccess: () => navigate({ name: 'home', id: relinkFor }) });
    else if (linkFor) linkTo.mutate(link, { onSuccess: () => navigate({ name: 'home', id: linkFor }) });
    else if (data.existing) navigate({ name: 'home', id: data.existing });
    else add.mutate(link, { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) });
  };
  const label = relinkFor
    ? t('join.useLink')
    : linkFor
      ? busy
        ? t('join.linking')
        : t('join.link')
      : data?.existing
        ? t('join.open')
        : busy
          ? t('join.adding')
          : t('join.add');
  const title = relinkFor ? t('join.relinkTitle') : linkFor ? t('join.linkTitle') : t('join.title');

  return (
    <ScreenFrame title={title}>
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
        {gameMove && (
          <p role="alert" className="flex items-start gap-2 font-semibold">
            <WarningDiamond className="mt-0.5 h-5 w-5 flex-none text-coral" aria-hidden />
            {gameMove.loader
              ? t('join.linkVersion', { version: gameMove.version, loader: loaderLabel(gameMove.loader) })
              : t('join.linkVersionVanilla', { version: gameMove.version })}
          </p>
        )}
        {data && linkFor && <p>{t('join.linkNote')}</p>}
        {add.isError && <ErrorCard error={add.error} />}
        {relink.isError && <ErrorCard error={relink.error} />}
        {linkTo.isError && <ErrorCard error={linkTo.error} />}
        {/* Cancel is always here, so a link that fails (or a deep link the player didn't mean) has a way out. */}
        <div className="sticky bottom-0 flex items-center justify-end gap-4 bg-ground py-4 shadow-[0_-1px_0_var(--color-line)]">
          {data && !relinkFor && !linkFor && !data.existing && <p className="text-sm text-ink-dim">{t('join.note')}</p>}
          <Button onClick={() => navigate({ name: 'home', id: relinkFor ?? linkFor ?? null })}>{t('common.cancel')}</Button>
          {data && (
            <Button variant="primary" size="lg" disabled={busy} onClick={go}>
              {label}
            </Button>
          )}
        </div>
      </div>
    </ScreenFrame>
  );
}
