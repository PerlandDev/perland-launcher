import { useTranslation } from 'react-i18next';
import { navigate } from '../../state/route';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { Medals } from './Medals';

/** The title bar's trophy: the medal case, then the achievements earned on servers made on Perland. */
export function AchievementsScreen() {
  const { t } = useTranslation();
  return (
    <ScreenFrame title={t('medals.screenTitle')} onBack={() => navigate({ name: 'home', id: null })}>
      <div className="max-w-2xl">
        <Medals />
      </div>
    </ScreenFrame>
  );
}
