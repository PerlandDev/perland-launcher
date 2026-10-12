import { useTranslation } from 'react-i18next';
import { useActiveAccountName } from '../../state/queries';
import { Lock } from '../../ui/Icon';

/** The invite-only line: advice in the info hue, led by a lock. Over marquee art it sits on a plate. */
export function InviteOnly({ onArt = false }: { onArt?: boolean }) {
  const { t } = useTranslation();
  const account = useActiveAccountName();
  return (
    <p className={onArt ? 'invite-only marquee-plate' : 'invite-only'}>
      <Lock aria-hidden />
      {account ? t('join.inviteOnly', { name: account }) : t('join.inviteOnlyNoAccount')}
    </p>
  );
}
