import { useTranslation } from 'react-i18next';
import { navigate } from '../../state/route';

/** A quiet plate: the instance still plays and joins the last address; this offers a new link. */
export function DeadLinkPlate({ id }: { id: string }) {
  const { t } = useTranslation();
  return (
    <p className="marquee-plate dead-link" role="status">
      {t('server.deadLink')}
      {' · '}
      <button type="button" className="underline underline-offset-4" onClick={() => navigate({ name: 'joinServer', invite: '', relinkFor: id })}>
        {t('server.useNewLink')}
      </button>
    </p>
  );
}
