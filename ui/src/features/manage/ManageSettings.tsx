import { useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { isLauncherError } from '../../ipc/errors';
import type { ManageSettings as Saved, ManageSettingsSave } from '../../ipc/schema';
import { openManagePanel, useManageDetail, useManageSettings, useManageSettingsSave } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { ExternalLink, Lock } from '../../ui/Icon';
import { Select } from '../../ui/Select';
import { SwitchRow } from '../../ui/SwitchRow';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { Motd } from '../servers/MotdText';

const MODES = ['survival', 'creative', 'adventure', 'spectator'];
const DIFFICULTIES = ['peaceful', 'easy', 'normal', 'hard'];
/** The fields a plan can lock here, by the Engine's name for them. */
type FieldName = 'motd' | 'mode' | 'difficulty' | 'maxPlayers' | 'pvp' | 'enableWhitelist' | 'version';
const FIELDS: readonly string[] = ['motd', 'mode', 'difficulty', 'maxPlayers', 'pvp', 'enableWhitelist', 'version'];

interface Draft {
  motd: string;
  mode: string;
  difficulty: string;
  maxPlayers: string;
  pvp: boolean;
  enableWhitelist: boolean;
  version: string;
}
const draftOf = (s: Saved): Draft => ({
  motd: s.motd,
  mode: s.mode,
  difficulty: s.difficulty,
  maxPlayers: s.maxPlayers === null ? '' : String(s.maxPlayers),
  pvp: s.pvp,
  enableWhitelist: s.enableWhitelist,
  version: s.version,
});

type Apply = ManageSettingsSave['apply'];
type Notice = 'saved' | 'savedRestarted' | 'savedPending' | 'changed';

/** A choice the Engine knows, else the saved value as it stands, so a select never shows a value it can't hold. */
const options = (known: string[], current: string, label: (value: string) => string) =>
  (known.includes(current) ? known : [...known, current]).map((value) => ({ value, label: known.includes(value) ? label(value) : value }));

/** Under a field the plan locks: the reason and Upgrade, which opens the panel's plans page. */
function LockedNote({ tenantId }: { tenantId: string }) {
  const { t } = useTranslation();
  const once = useSingleFlight();
  return (
    <div className="manage-locked">
      <Lock className="h-4 w-4 flex-none" aria-hidden />
      <p className="flex-1">{t('manage.settings.locked')}</p>
      <Button onClick={() => once((done) => void openManagePanel(tenantId, null).finally(done))}>{t('manage.settings.upgrade')}</Button>
    </div>
  );
}

function SettingsForm({
  tenantId,
  stack,
  saved,
  running,
  moving,
  onSaved,
  onChanged,
}: {
  tenantId: string;
  stack: string;
  saved: Saved;
  running: boolean;
  /** Starting or stopping: a save waits for the next start rather than restart it mid-change. */
  moving: boolean;
  onSaved: (notice: Notice) => void;
  onChanged: () => void;
}) {
  const { t } = useTranslation();
  const save = useManageSettingsSave(tenantId, stack, { inline: true });
  const [draft, setDraft] = useState<Draft>(() => draftOf(saved));
  const [apply, setApply] = useState<Apply>('next_restart');
  const [locked, setLocked] = useState<ReadonlySet<FieldName>>(new Set());
  const [asking, setAsking] = useState<'version' | 'dataLoss' | null>(null);
  const [failure, setFailure] = useState<unknown>(null);
  const [panelOnly, setPanelOnly] = useState(false);
  // The body the Engine asked about, sent again with its confirmation.
  const last = useRef<ManageSettingsSave | null>(null);
  const panelOnce = useSingleFlight();

  const edit = (patch: Partial<Draft>) => setDraft((d) => ({ ...d, ...patch }));
  const max = draft.maxPlayers.trim() === '' ? null : Number(draft.maxPlayers);
  const maxInvalid = max !== null && (!Number.isInteger(max) || max < 1 || max > 1000);

  const send = (body: ManageSettingsSave) => {
    if (save.isPending) return;
    last.current = body;
    setFailure(null);
    setPanelOnly(false);
    save.mutate(body, {
      onSuccess: (answer) => onSaved(answer.restarted ? 'savedRestarted' : answer.pending ? 'savedPending' : 'saved'),
      onError: (error) => {
        const code = isLauncherError(error) ? error.code : null;
        if (code === 'version_change_needs_confirmation') return setAsking('version');
        if (code === 'data_loss_needs_confirmation') return setAsking('dataLoss');
        if (code === 'settings_changed') return onChanged();
        const field = isLauncherError(error) && code === 'plan_feature_locked' ? error.params.field : undefined;
        if (field !== undefined && FIELDS.includes(field)) {
          const name = field as FieldName;
          setLocked((all) => new Set(all).add(name));
          // The field goes back to what is saved: the plan keeps it there.
          edit({ [name]: draftOf(saved)[name] });
          return;
        }
        // A refusal of something the form doesn't edit (an Engine that refuses the modpack's own `env` back): Upgrade
        // can't help, only the panel can change it.
        if (code === 'plan_feature_locked') return setPanelOnly(true);
        setFailure(error);
      },
    });
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (maxInvalid) return;
    send({
      revision: saved.revision,
      motd: saved.motdReadOnly || draft.motd === saved.motd ? null : draft.motd,
      mode: draft.mode,
      difficulty: draft.difficulty,
      maxPlayers: max,
      pvp: draft.pvp,
      enableWhitelist: draft.enableWhitelist,
      version: saved.versionLocked || draft.version.trim() === saved.version ? null : draft.version.trim(),
      apply: running ? apply : moving ? 'next_restart' : 'now',
    });
  };
  const confirmAgain = (extra: Partial<ManageSettingsSave>) => {
    setAsking(null);
    if (last.current) send({ ...last.current, ...extra });
  };

  const isLocked = (name: FieldName) => locked.has(name);
  const note = (name: FieldName) => isLocked(name) && <LockedNote tenantId={tenantId} />;

  return (
    <form className="grid gap-8" onSubmit={submit} noValidate>
      {saved.motdReadOnly ? (
        <div className="grid gap-1.5">
          <p className="text-sm font-semibold">{t('manage.settings.motd')}</p>
          <div className="well manage-motd">
            <Motd text={saved.motd} />
          </div>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-sm text-ink-dim">{t('manage.settings.motdReadOnly')}</p>
            <Button icon={<ExternalLink />} onClick={() => panelOnce((done) => void openManagePanel(tenantId, stack).finally(done))}>
              {t('manage.settings.editInPerland')}
            </Button>
          </div>
        </div>
      ) : (
        <div className="grid gap-1.5">
          <label htmlFor="manage-motd" className="text-sm font-semibold">
            {t('manage.settings.motd')}
          </label>
          <input
            id="manage-motd"
            className="field-input"
            aria-describedby="manage-motd-hint"
            maxLength={256}
            disabled={isLocked('motd')}
            value={draft.motd}
            onChange={(event) => edit({ motd: event.target.value.replace(/[\r\n]/g, ' ') })}
          />
          <p id="manage-motd-hint" className="text-sm text-ink-dim">
            {t('manage.settings.motdHint')}
          </p>
          {note('motd')}
        </div>
      )}
      <div className="manage-pair">
        <div className="grid gap-2">
          <Select
            label={t('manage.settings.mode')}
            value={draft.mode}
            disabled={isLocked('mode')}
            options={options(MODES, draft.mode, (v) => t(`manage.settings.modes.${v}`))}
            onChange={(mode) => edit({ mode })}
          />
          {note('mode')}
        </div>
        <div className="grid gap-2">
          <Select
            label={t('manage.settings.difficulty')}
            value={draft.difficulty}
            disabled={isLocked('difficulty')}
            options={options(DIFFICULTIES, draft.difficulty, (v) => t(`manage.settings.difficulties.${v}`))}
            onChange={(difficulty) => edit({ difficulty })}
          />
          {note('difficulty')}
        </div>
      </div>
      <div className="grid gap-1.5">
        <label htmlFor="manage-max" className="text-sm font-semibold">
          {t('manage.settings.maxPlayers')}
        </label>
        <input
          id="manage-max"
          type="number"
          inputMode="numeric"
          min={1}
          max={1000}
          className="field-input w-32 tabular-nums"
          aria-invalid={maxInvalid || undefined}
          aria-describedby={maxInvalid ? 'manage-max-error' : undefined}
          disabled={isLocked('maxPlayers')}
          value={draft.maxPlayers}
          onChange={(event) => edit({ maxPlayers: event.target.value })}
        />
        {maxInvalid && (
          <p id="manage-max-error" className="text-sm text-coral">
            {t('manage.settings.maxPlayersInvalid')}
          </p>
        )}
        {note('maxPlayers')}
      </div>
      <SwitchRow
        label={t('manage.settings.pvp')}
        hint={t('manage.settings.pvpHint')}
        checked={draft.pvp}
        disabled={isLocked('pvp')}
        onChange={(pvp) => edit({ pvp })}
        after={note('pvp')}
      />
      <SwitchRow
        label={t('manage.settings.whitelist')}
        hint={t('manage.settings.whitelistHint')}
        checked={draft.enableWhitelist}
        disabled={isLocked('enableWhitelist')}
        onChange={(enableWhitelist) => edit({ enableWhitelist })}
        after={note('enableWhitelist')}
      />
      {!saved.versionLocked && (
        <div className="grid gap-1.5">
          <label htmlFor="manage-version" className="text-sm font-semibold">
            {t('manage.settings.version')}
          </label>
          <input
            id="manage-version"
            className="field-input w-48 font-mono"
            aria-describedby="manage-version-hint"
            autoComplete="off"
            spellCheck={false}
            disabled={isLocked('version')}
            value={draft.version}
            onChange={(event) => edit({ version: event.target.value })}
          />
          <p id="manage-version-hint" className="text-sm text-ink-dim">
            {t('manage.settings.versionHint')}
          </p>
          {note('version')}
        </div>
      )}
      {failure !== null && <ErrorCard error={failure} />}
      {panelOnly && (
        <div className="note-plate items-center" role="alert">
          <p className="flex-1">{t('manage.settings.panelOnly')}</p>
          <Button icon={<ExternalLink />} onClick={() => panelOnce((done) => void openManagePanel(tenantId, stack).finally(done))}>
            {t('manage.morePerland')}
          </Button>
        </div>
      )}
      <div className="sticky bottom-0 flex flex-wrap items-center justify-end gap-x-6 gap-y-3 bg-ground py-4 shadow-[0_-1px_0_var(--color-line)]">
        {running && (
          <fieldset className="flex flex-wrap items-center gap-2">
            <legend className="sr-only">{t('manage.settings.applyLegend')}</legend>
            {(['now', 'next_restart'] as const).map((choice) => (
              <label key={choice} className="seg" data-checked={apply === choice ? '' : undefined}>
                <input type="radio" name="manage-apply" className="sr-only" checked={apply === choice} onChange={() => setApply(choice)} />
                {choice === 'now' ? t('manage.settings.applyNow') : t('manage.settings.applyLater')}
              </label>
            ))}
          </fieldset>
        )}
        <Button type="submit" size="lg" aria-disabled={save.isPending || undefined}>
          {save.isPending ? t('common.saving') : t('common.save')}
        </Button>
      </div>
      {asking === 'version' && (
        <Dialog
          open
          onClose={() => setAsking(null)}
          title={t('manage.settings.versionTitle')}
          actions={
            <>
              <Button onClick={() => setAsking(null)}>{t('common.cancel')}</Button>
              <Button variant="danger" onClick={() => confirmAgain({ confirmVersionChange: true })}>
                {t('manage.settings.versionConfirm')}
              </Button>
            </>
          }
        >
          <p>{t('manage.settings.versionBody')}</p>
        </Dialog>
      )}
      {asking === 'dataLoss' && (
        <Dialog
          open
          onClose={() => setAsking(null)}
          title={t('manage.settings.dataLossTitle')}
          actions={
            <>
              <Button onClick={() => setAsking(null)}>{t('common.cancel')}</Button>
              <Button variant="danger" onClick={() => confirmAgain({ confirmVersionChange: true, acknowledgeDataLoss: true })}>
                {t('manage.settings.dataLossConfirm')}
              </Button>
            </>
          }
        >
          <p>{t('manage.settings.dataLossBody')}</p>
        </Dialog>
      )}
    </form>
  );
}

/** Settings: only what a player changes for friends (the server-list name, game mode, difficulty, max players, PvP, the
 *  whitelist and the Minecraft version). The panel keeps the rest. A field the plan locks shows locked with Upgrade. */
export function ManageSettings({ tenantId, stack }: { tenantId: string; stack: string }) {
  const { t } = useTranslation();
  const settings = useManageSettings(tenantId, stack);
  const detail = useManageDetail(tenantId, stack);
  const [notice, setNotice] = useState<Notice | null>(null);
  // Bumped when the settings are loaded again after someone else's change, so the form starts over from them.
  const [generation, setGeneration] = useState(0);

  if (settings.isError && !settings.data) {
    return <ErrorCard error={settings.error} actions={<Button onClick={() => void settings.refetch()}>{t('common.retry')}</Button>} />;
  }
  if (!settings.data) return <p className="text-ink-dim">{t('common.loading')}</p>;
  const noticeText = notice === 'changed' ? t('manage.settings.changed') : notice ? t(`manage.settings.${notice}`) : '';
  return (
    <div className="grid gap-6">
      <p role="status" className={noticeText ? 'note-plate' : 'sr-only'} data-lamp={notice === 'changed' ? undefined : 'none'}>
        {noticeText}
      </p>
      <SettingsForm
        key={`${settings.data.revision}-${generation}`}
        tenantId={tenantId}
        stack={stack}
        saved={settings.data}
        running={detail.data?.state === 'running'}
        moving={detail.data?.state === 'starting' || detail.data?.state === 'stopping'}
        onSaved={setNotice}
        onChanged={() => {
          setNotice('changed');
          void settings.refetch().then(() => setGeneration((n) => n + 1));
        }}
      />
    </div>
  );
}
