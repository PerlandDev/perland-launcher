import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';
import { applyLanguage } from '../../i18n';
import type { Settings } from '../../ipc/schema';
import { qk, useSaveSettings } from '../../state/queries';

/** How long a slider has to sit still before its value is saved. */
const SETTLE_MS = 400;

/**
 * Saves settings as the player changes them, without losing or reordering anything:
 * - `draft` is what the player has asked for and is shown until the backend has it, so a control never lags;
 * - one save is in flight at a time and edits made meanwhile go out together afterwards, so an older answer cannot overwrite a newer value;
 * - an edit made with `settle` waits for the player to stop (a dragged slider), and is still sent if the screen closes first;
 * - a failed save is shown by the save hook and the controls go back to what is stored.
 */
export function useSettingsDraft() {
  const qc = useQueryClient();
  const { mutateAsync } = useSaveSettings();
  const [draft, setDraft] = useState<Settings | null>(null);
  const [saved, setSaved] = useState(false);
  const desired = useRef<Settings | null>(null);
  const dirty = useRef(false);
  const running = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const stopTimer = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  }, []);

  const flush = useCallback(async () => {
    if (running.current) return;
    running.current = true;
    try {
      while (dirty.current && !timer.current) {
        dirty.current = false;
        await mutateAsync(desired.current as Settings);
      }
      if (!dirty.current) {
        desired.current = null;
        setDraft(null);
        setSaved(true);
      }
    } catch {
      dirty.current = false;
      desired.current = null;
      stopTimer();
      setDraft(null);
      applyLanguage(qc.getQueryData<Settings>(qk.settings)?.language ?? null);
    } finally {
      running.current = false;
    }
  }, [mutateAsync, qc, stopTimer]);

  const edit = useCallback(
    (patch: Partial<Settings>, settle = false) => {
      const base = desired.current ?? qc.getQueryData<Settings>(qk.settings);
      if (!base) return;
      const next = { ...base, ...patch };
      desired.current = next;
      dirty.current = true;
      setDraft(next);
      setSaved(false);
      stopTimer();
      if (settle) {
        timer.current = setTimeout(() => {
          timer.current = null;
          void flush();
        }, SETTLE_MS);
      } else {
        void flush();
      }
    },
    [flush, qc, stopTimer],
  );

  useEffect(
    () => () => {
      if (!timer.current) return;
      stopTimer();
      void flush();
    },
    [flush, stopTimer],
  );

  return { draft, saved, edit };
}
