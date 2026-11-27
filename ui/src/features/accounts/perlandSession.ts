import type { QueryClient } from '@tanstack/react-query';
import type { TFunction } from 'i18next';
import type { PerlandAccount, PerlandStatus } from '../../ipc/schema';
import { qk } from '../../state/queries';

/** The name a Perland account goes by: its own, else its email, else "Perland player". */
export const perlandName = (account: PerlandAccount, t: TFunction) => account.name || account.email || t('perland.player');

/** Someone new is (or no one is) signed in to Perland: the status says so at once and reads again, and the old player's
 *  servers and numbers go, so they never show for the next one. */
export function setPerlandAccount(qc: QueryClient, account: PerlandAccount | null) {
  qc.setQueryData<PerlandStatus>(qk.perland, (old) => (old ? { ...old, account } : old));
  void qc.invalidateQueries({ queryKey: qk.perland });
  qc.removeQueries({ queryKey: qk.yourServers });
  qc.removeQueries({ queryKey: qk.perlandStats });
}
