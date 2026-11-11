import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { commands } from '../../ipc/commands';
import type { NoteGroup, ReleaseNotes } from '../../ipc/schema';
import { qk, useAppInfo, useWhatsNewSeen } from '../../state/queries';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { releaseNotesData } from './notes';
import { WhatsNewDialog } from './WhatsNewDialog';
import { PRIVACY_NOTE, showsNote, whatsNew, withNote } from './whatsNewRules';

/** How long after the first screen loads the dialog waits, so it doesn't race whatever the launch opens (a join link). */
export const SETTLE_MS = 1500;

/** `?whatsNew` in the mocked browser build opens the dialog with every version, to look at it. */
const preview = () => import.meta.env.MODE === 'mock' && new URLSearchParams(window.location.search).has('whatsNew');

/** Once per launch: show the release notes this player has not seen (design: What's new). The marker moves when
 *  the dialog opens, or at once when there is nothing to show. If the player is no longer on Home, another dialog is
 *  open or a game is starting when its turn comes, nothing opens and the marker stays, so the next launch tries again. */
export function WhatsNewOnLaunch({
  firstRunDone,
  notes = releaseNotesData,
  settleMs = SETTLE_MS,
}: {
  /** From the settings App already holds: a second reader here would refetch them behind App's own error screen. */
  firstRunDone: boolean;
  notes?: ReleaseNotes;
  settleMs?: number;
}) {
  const qc = useQueryClient();
  const appInfo = useAppInfo().data;
  const seen = useWhatsNewSeen();
  const [groups, setGroups] = useState<NoteGroup[] | null>(() => (preview() ? notes.groups : null));
  const checked = useRef(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const loaded = appInfo !== undefined && seen.isSuccess;
  useEffect(() => {
    if (!loaded || checked.current) return;
    checked.current = true;
    if (preview()) return;
    const mark = () =>
      void commands
        .whatsNewMark()
        .then(() => qc.setQueryData(qk.whatsNewSeen, appInfo.version))
        .catch(() => {});
    const result = whatsNew({ notes, running: appInfo.version, seen: seen.data, firstRunDone, dev: appInfo.build === 'dev' });
    if (result.mark === 'now') mark();
    const dev = appInfo.build === 'dev';
    const show = (shown: NoteGroup[], moveMarker: boolean) => {
      if (shown.length === 0) return;
      // Not cleared on cleanup: the check runs once, and a strict-mode remount must not cancel it.
      setTimeout(() => {
        // Its turn only on the screen every launch starts on: a perland:// link has by now opened Join server or New
        // instance, which are screens, not dialogs.
        if (!alive.current || routeSnapshot().name !== 'home' || document.querySelector('dialog[open]') || runStore.active().length > 0) return;
        setGroups(shown);
        if (moveMarker) mark();
        // The player has now been told about anonymous data: the launcher may share it from its next start.
        if (showsNote(shown, PRIVACY_NOTE)) void commands.telemetryNoticeShown().catch(() => {});
      }, settleMs);
    };
    // Nothing about anonymous data is sent until the player was shown its note. An updated launcher that would not
    // show it by the usual rule (no marker, a skipped release) shows it anyway, once.
    void (firstRunDone && !dev ? commands.telemetryNoticeDue().catch(() => false) : Promise.resolve(false)).then((due) =>
      show(due ? withNote(result.groups, notes, appInfo.version, PRIVACY_NOTE) : result.groups, result.groups.length > 0),
    );
  }, [loaded, notes, appInfo, firstRunDone, seen.data, settleMs, qc]);

  return <WhatsNewDialog open={groups !== null} groups={groups ?? []} all={notes.groups} onClose={() => setGroups(null)} />;
}
