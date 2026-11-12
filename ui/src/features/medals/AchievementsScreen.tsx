import { useTranslation } from 'react-i18next';
import { navigate } from '../../state/route';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { Medals } from './Medals';

/** Medals and achievements, the title bar's trophy tab (the Profile's overview opens it too): the medals, then the
 *  Minecraft achievements earned on servers made on Perland. */
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
