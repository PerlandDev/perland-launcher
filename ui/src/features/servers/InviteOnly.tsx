import { useTranslation } from 'react-i18next';
import { useActiveAccountName } from '../../state/queries';
import { Lock } from '../../ui/Icon';

/** The invite-only line: advice in the info hue, led by a lock. Over marquee art it sits on a plate. The server's `owner` is
 *  told who gets in rather than to ask the owner. */
export function InviteOnly({ onArt = false, owner = false }: { onArt?: boolean; owner?: boolean }) {
  const { t } = useTranslation();
  const account = useActiveAccountName();
  const line = owner ? t('join.inviteOnlyOwner') : account ? t('join.inviteOnly', { name: account }) : t('join.inviteOnlyNoAccount');
  return (
    <p className={onArt ? 'invite-only marquee-plate' : 'invite-only'}>
      <Lock aria-hidden />
      {line}
    </p>
  );
}
