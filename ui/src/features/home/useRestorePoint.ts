import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef } from 'react';
import { commands } from '../../ipc/commands';
import { qk } from '../../state/queries';
import { useRun } from '../../state/runStore';

/** The instance's restore point. A run can take, seal or mark it, so it is asked again each time the run ends. */
export function useRestorePoint(id: string) {
  const qc = useQueryClient();
  const state = useRun(id).state;
  const last = useRef(state);
  useEffect(() => {
    if (state === 'idle' && last.current !== 'idle') void qc.invalidateQueries({ queryKey: qk.restorePoint(id) });
    last.current = state;
  }, [qc, id, state]);
  return useQuery({ queryKey: qk.restorePoint(id), queryFn: () => commands.restorePoint(id) });
}
