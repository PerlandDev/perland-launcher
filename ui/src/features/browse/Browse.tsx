import { useTranslation } from 'react-i18next';
import type { Route } from '../../state/route';
import { ScreenFrame } from '../../ui/ScreenFrame';

export type BrowseRoute = Extract<Route, { name: 'browse' }>;

export function Browse({ route }: { route: BrowseRoute }) {
  const { t } = useTranslation();
  return (
    <ScreenFrame title={t('browse.title')} wide>
      {route.kind}
    </ScreenFrame>
  );
}
