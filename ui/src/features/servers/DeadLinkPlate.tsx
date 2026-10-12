import { useTranslation } from 'react-i18next';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Unlink } from '../../ui/Icon';

/** A quiet plate: the instance still plays and joins the last address; this offers a new link. It has
 *  the update plate's anatomy (a raised action, never lit), led by a broken-link icon, not a fault. */
export function DeadLinkPlate({ id }: { id: string }) {
  const { t } = useTranslation();
  return (
    <p className="marquee-plate plate-row dead-link" role="status">
      <Unlink aria-hidden />
      <span>{t('server.deadLink')}</span>
      <Button onClick={() => navigate({ name: 'joinServer', invite: '', relinkFor: id })}>{t('server.useNewLink')}</Button>
    </p>
  );
}
