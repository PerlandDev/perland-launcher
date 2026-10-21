import { useEffect, useLayoutEffect, useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { isLauncherError } from '../../ipc/errors';
import { manageLog, useManageLog } from '../../state/manageLog';
import { useManageConsole, useManageDetail } from '../../state/queries';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';

/** How close to the bottom (px) still counts as following the newest line. */
const FOLLOW_SLACK = 40;

/** The live lines, newest at the bottom. It follows them until the player scrolls up, then offers Jump to latest. Not a
 *  live region (`aria-live="off"`): a busy server would talk over everything. */
function LogLines({ lines }: { lines: string[] }) {
  const { t } = useTranslation();
  const scroller = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && follow) el.scrollTop = el.scrollHeight;
  }, [follow, lines]);
  return (
    <div className="relative">
      <div
        ref={scroller}
        role="log"
        aria-label={t('manage.console.log')}
        aria-live="off"
        tabIndex={0}
        className="manage-log selectable"
        onScroll={(event) => {
          const el = event.currentTarget;
          setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < FOLLOW_SLACK);
        }}
      >
        {lines.length === 0 ? (
          <p className="text-ink-dim">{t('manage.console.empty')}</p>
        ) : (
          lines.map((line, index) => <div key={index}>{line}</div>)
        )}
      </div>
      {!follow && (
        <Button className="absolute right-4 bottom-4" onClick={() => setFollow(true)}>
          {t('manage.console.jumpToLatest')}
        </Button>
      )}
    </div>
  );
}

/** Console: the server's live log and a box for one command. A command that stops the server asks first and is sent again
 *  with the confirmation. The log stream is opened while this is on screen and the server runs, and closed after. */
export function ManageConsole({ tenantId, stack }: { tenantId: string; stack: string }) {
  const { t } = useTranslation();
  const detail = useManageDetail(tenantId, stack);
  const running = detail.data?.state === 'running';
  const log = useManageLog(tenantId, stack);
  const send = useManageConsole(tenantId, stack, { inline: true });
  const [command, setCommand] = useState('');
  const [asking, setAsking] = useState<string | null>(null);
  // Bumped by Reconnect, so the stream opens again.
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!running) return;
    manageLog.resume(tenantId, stack);
    commands.manageConsoleOpen(tenantId, stack).catch(pushError);
    return () => {
      void commands.manageConsoleClose(tenantId, stack).catch(() => {});
    };
  }, [tenantId, stack, running, attempt]);

  const run = (text: string, confirm: boolean) => {
    if (send.isPending) return;
    send.mutate(
      { command: text, confirm },
      {
        onSuccess: () => setCommand(''),
        onError: (error) => {
          if (isLauncherError(error) && error.code === 'command_needs_confirmation') setAsking(text);
        },
      },
    );
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const text = command.trim().replace(/^\//, '');
    if (text !== '') run(text, false);
  };

  const refusal = send.isError && !(isLauncherError(send.error) && send.error.code === 'command_needs_confirmation') ? send.error : null;

  return (
    <div className="grid gap-6">
      {(running || log.lines.length > 0) && <LogLines lines={log.lines} />}
      {running && log.ended && (
        <div className="note-plate items-center" data-lamp="none">
          <p className="flex-1">{t('manage.console.ended')}</p>
          <Button onClick={() => setAttempt((n) => n + 1)}>{t('manage.console.reconnect')}</Button>
        </div>
      )}
      {detail.data && !running && <p className="empty-slot">{t('manage.console.notRunning')}</p>}
      {running && (
        <form className="grid gap-1.5" onSubmit={submit}>
          <label htmlFor="manage-command" className="text-sm font-semibold">
            {t('manage.console.command')}
          </label>
          <div className="flex gap-3">
            <input
              id="manage-command"
              className="field-input min-w-0 flex-1 font-mono"
              aria-describedby="manage-command-hint"
              autoComplete="off"
              spellCheck={false}
              value={command}
              onChange={(event) => setCommand(event.target.value)}
            />
            <Button type="submit" aria-disabled={send.isPending || undefined}>
              {send.isPending ? t('manage.console.sending') : t('manage.console.send')}
            </Button>
          </div>
          <p id="manage-command-hint" className="text-sm text-ink-dim">
            {t('manage.console.commandHint')}
          </p>
        </form>
      )}
      {send.isSuccess && (
        <div className="grid gap-1.5" role="status">
          <p className="text-sm font-semibold">{t('manage.console.reply')}</p>
          {send.data.output.trim() === '' ? (
            <p className="text-ink-dim">{t('manage.console.noReply')}</p>
          ) : (
            <pre className="well selectable manage-reply">{send.data.output}</pre>
          )}
        </div>
      )}
      {refusal !== null && <ErrorCard error={refusal} />}
      {asking !== null && (
        <Dialog
          open
          onClose={() => setAsking(null)}
          title={t('manage.console.confirmTitle')}
          actions={
            <>
              <Button onClick={() => setAsking(null)}>{t('common.cancel')}</Button>
              <Button
                variant="danger"
                onClick={() => {
                  setAsking(null);
                  run(asking, true);
                }}
              >
                {t('manage.console.confirm')}
              </Button>
            </>
          }
        >
          <p>{t('manage.console.confirmBody')}</p>
        </Dialog>
      )}
    </div>
  );
}
