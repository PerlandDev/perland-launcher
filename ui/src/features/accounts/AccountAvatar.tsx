import { Avatar } from '../../art/Avatar';
import type { Account } from '../../ipc/schema';
import { useAccountFace } from '../../state/queries';

/** An account's picture: a Microsoft account's own skin face once it arrives, the generated face until then and for offline accounts. */
export function AccountAvatar({ account, size }: { account: Pick<Account, 'provider' | 'uuid'>; size?: number }) {
  const face = useAccountFace(account);
  return <Avatar uuid={account.uuid} size={size} face={face.data ?? null} />;
}
