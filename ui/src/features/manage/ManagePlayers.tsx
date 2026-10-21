import { useId, useState, type FormEvent, type ReactNode } from 'react';
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import type { ManageChangeResult, ManageList } from '../../ipc/schema';
import { useManageDetail, useManagePlayers, useManagePlayersChange, useManageSettings } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';

/** The most names one change takes (the Engine's limit). */
const MAX_NAMES = 50;

/** Names typed into a box: spaces, commas and new lines separate them. */
const splitNames = (text: string) => text.split(/[\s,]+/).filter((name) => name !== '');

/** What a change did to one name, in a sentence. A newer Engine word reads as plain "sent". */
function resultLine(t: TFunction, result: ManageChangeResult, has: (key: string) => boolean): string {
  if (result.status === 'failed' && result.message) return t('manage.result.failedWith', { name: result.name, message: result.message });
  const key = `manage.result.${result.status}`;
  return has(key) ? t(key, { name: result.name }) : t('manage.result.other', { name: result.name });
}

/** One list on the Players tab: a 700 heading, its rows split by seams, then what is needed to add to it. */
function ListSection({ title, hint, children }: { title: string; hint?: ReactNode; children: ReactNode }) {
  const id = useId();
  return (
    <section className="manage-list" aria-labelledby={id}>
      <div className="grid gap-1">
        <h2 id={id} className="text-lg font-bold">
          {title}
        </h2>
        {hint}
      </div>
      {children}
    </section>
  );
}

/** Rows of names, each with its own action at right; `empty` when there are none. */
function NameRows({ names, empty, children }: { names: string[]; empty: string; children: (name: string) => ReactNode }) {
  if (names.length === 0) return <p className="text-sm text-ink-dim">{empty}</p>;
  return (
    <ul className="manage-rows">
      {names.map((name) => (
        <li key={name} className="manage-row">
          <span className="manage-row-name">{name}</span>
          {children(name)}
        </li>
      ))}
    </ul>
  );
}

/** A box to type names into and the key that sends them. The box clears once the change went through. */
function AddBox({ label, button, busy, onAdd }: { label: string; button: string; busy: boolean; onAdd: (names: string[]) => Promise<boolean> }) {
  const { t } = useTranslation();
  const id = useId();
  const [text, setText] = useState('');
  const [tooMany, setTooMany] = useState(false);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    const names = splitNames(text);
    if (names.length === 0) return;
    if (names.length > MAX_NAMES) return setTooMany(true);
    setTooMany(false);
    void onAdd(names).then((done) => {
      if (done) setText('');
    });
  };
  return (
    <form className="manage-add" onSubmit={submit}>
      <input
        className="field-input min-w-0 flex-1 sm:max-w-[22rem]"
        placeholder={t('manage.players.namePlaceholder')}
        aria-label={label}
        aria-describedby={`${id}-note`}
        aria-invalid={tooMany || undefined}
        autoComplete="off"
        spellCheck={false}
        value={text}
        onChange={(event) => setText(event.target.value)}
      />
      <Button type="submit" aria-disabled={busy || undefined}>
        {button}
      </Button>
      <p id={`${id}-note`} className={tooMany ? 'basis-full text-sm text-coral' : 'sr-only'}>
        {tooMany ? t('manage.players.tooMany') : t('manage.players.namesHint')}
      </p>
    </form>
  );
}

/** What the last change of a list did, one sentence per name. A live region, so the result is heard. */
function Results({ results }: { results: ManageChangeResult[] }) {
  const { t, i18n } = useTranslation();
  return (
    <div role="status" className="grid gap-1 empty:hidden">
      {results.map((result, index) => (
        <p key={`${result.name}-${index}`} className="text-sm font-semibold">
          {resultLine(t, result, (key) => i18n.exists(key))}
        </p>
      ))}
    </div>
  );
}

/** Players: who is on (Kick), and the whitelist, admins and bans with add and remove. Every label is a plain word. A stopped
 *  server keeps the whitelist and admins editable (the Engine writes the files), while who is on and bans need it running. */
export function ManagePlayers({ tenantId, stack, onOpenSettings }: { tenantId: string; stack: string; onOpenSettings: () => void }) {
  const { t } = useTranslation();
  const detail = useManageDetail(tenantId, stack);
  const players = useManagePlayers(tenantId, stack);
  const settings = useManageSettings(tenantId, stack);
  const change = useManagePlayersChange(tenantId, stack, { inline: true });
  const [results, setResults] = useState<Partial<Record<ManageList, ManageChangeResult[]>>>({});
  const [failure, setFailure] = useState<{ list: ManageList; error: unknown } | null>(null);
  // The ban waiting for its confirmation; `settle` tells the box whether to clear.
  const [banning, setBanning] = useState<{ names: string[]; settle: (done: boolean) => void } | null>(null);

  const send = (list: ManageList, add: boolean, names: string[]) =>
    new Promise<boolean>((resolve) => {
      if (change.isPending) return resolve(false);
      setFailure(null);
      change.mutate(
        { list, add, names },
        {
          onSuccess: (answer) => {
            setResults((all) => ({ ...all, [list]: answer }));
            resolve(true);
          },
          onError: (error) => {
            setFailure({ list, error });
            resolve(false);
          },
        },
      );
    });

  if (players.isError && !players.data) {
    return <ErrorCard error={players.error} actions={<Button onClick={() => void players.refetch()}>{t('common.retry')}</Button>} />;
  }
  if (!players.data) return <p className="text-ink-dim">{t('manage.players.loading')}</p>;

  const data = players.data;
  const running = data.running;
  const busy = change.isPending;
  const errorOf = (list: ManageList) => failure?.list === list && <ErrorCard error={failure.error} />;
  const whitelistOn = settings.data?.enableWhitelist;

  return (
    <div className="grid gap-10">
      {data.unreadable && (
        <p className="text-ink-dim">{detail.data?.state === 'starting' ? t('manage.players.starting') : t('manage.players.unreadable')}</p>
      )}
      {running && (
        <ListSection title={t('manage.players.online')}>
          <NameRows names={data.online} empty={t('manage.players.onlineEmpty')}>
            {(name) => (
              <Button variant="ghost" aria-disabled={busy || undefined} aria-label={t('manage.players.kickName', { name })} onClick={() => void send('online', false, [name])}>
                {t('manage.players.kick')}
              </Button>
            )}
          </NameRows>
          <Results results={results.online ?? []} />
          {errorOf('online')}
        </ListSection>
      )}
      <ListSection
        title={t('manage.players.whitelist')}
        hint={
          whitelistOn !== undefined && (
            <button type="button" className="manage-link" onClick={onOpenSettings}>
              {whitelistOn ? t('manage.players.whitelistOn') : t('manage.players.whitelistOff')}
            </button>
          )
        }
      >
        <NameRows names={data.whitelist} empty={t('manage.players.whitelistEmpty')}>
          {(name) => (
            <Button variant="ghost" aria-disabled={busy || undefined} aria-label={t('manage.players.removeName', { name })} onClick={() => void send('whitelist', false, [name])}>
              {t('manage.players.remove')}
            </Button>
          )}
        </NameRows>
        <AddBox label={t('manage.players.whitelistAdd')} button={t('manage.players.add')} busy={busy} onAdd={(names) => send('whitelist', true, names)} />
        <Results results={results.whitelist ?? []} />
        {errorOf('whitelist')}
      </ListSection>
      <ListSection title={t('manage.players.admins')} hint={<p className="text-sm text-ink-dim">{t('manage.players.adminsHint')}</p>}>
        <NameRows names={data.admins} empty={t('manage.players.adminsEmpty')}>
          {(name) => (
            <Button variant="ghost" aria-disabled={busy || undefined} aria-label={t('manage.players.removeAdminName', { name })} onClick={() => void send('admins', false, [name])}>
              {t('manage.players.removeAdmin')}
            </Button>
          )}
        </NameRows>
        <AddBox label={t('manage.players.makeAdmin')} button={t('manage.players.makeAdmin')} busy={busy} onAdd={(names) => send('admins', true, names)} />
        <Results results={results.admins ?? []} />
        {errorOf('admins')}
      </ListSection>
      <ListSection title={t('manage.players.bans')}>
        <NameRows names={data.bans.map((ban) => ban.name)} empty={t('manage.players.bansEmpty')}>
          {(name) =>
            running && (
              <Button variant="ghost" aria-disabled={busy || undefined} aria-label={t('manage.players.unbanName', { name })} onClick={() => void send('bans', false, [name])}>
                {t('manage.players.unban')}
              </Button>
            )
          }
        </NameRows>
        {running ? (
          <AddBox
            label={t('manage.players.banAdd')}
            button={t('manage.players.ban')}
            busy={busy}
            // A ban sends the player out: ask first. The box keeps the names until the ban went through.
            onAdd={(names) => new Promise<boolean>((settle) => setBanning({ names, settle }))}
          />
        ) : (
          <p className="text-sm text-ink-dim">{t('manage.players.bansNeedRunning')}</p>
        )}
        <Results results={results.bans ?? []} />
        {errorOf('bans')}
      </ListSection>
      {banning && (
        <Dialog
          open
          onClose={() => {
            banning.settle(false);
            setBanning(null);
          }}
          title={
            banning.names.length === 1
              ? t('manage.players.banTitle', { name: banning.names[0] })
              : t('manage.players.banTitleMany', { count: banning.names.length })
          }
          actions={
            <>
              <Button
                onClick={() => {
                  banning.settle(false);
                  setBanning(null);
                }}
              >
                {t('common.cancel')}
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  setBanning(null);
                  void send('bans', true, banning.names).then(banning.settle);
                }}
              >
                {t('manage.players.ban')}
              </Button>
            </>
          }
        >
          <p>{t('manage.players.banBody')}</p>
        </Dialog>
      )}
    </div>
  );
}
