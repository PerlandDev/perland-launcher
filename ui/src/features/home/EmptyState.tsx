import { useTranslation } from 'react-i18next';
import mark from '../../assets/mark.svg';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Plus, Search } from '../../ui/Icon';

export function EmptyState() {
  const { t } = useTranslation();
  return (
    <div className="grid h-full place-items-center p-10">
      <div className="grid max-w-md justify-items-center gap-5 text-center">
        <img src={mark} alt="" width={140} height={120} />
        <h1 className="font-display text-3xl">{t('home.emptyTitle')}</h1>
        <p className="text-ink-dim">{t('home.emptyBody')}</p>
        <div className="flex flex-wrap items-center justify-center gap-3">
          <Button variant="primary" size="lg" icon={<Plus />} onClick={() => navigate({ name: 'newInstance' })}>
            {t('home.emptyAction')}
          </Button>
          <Button size="lg" icon={<Search />} onClick={() => navigate({ name: 'browse', kind: 'modpack' })}>
            {t('browse.browseModpacks')}
          </Button>
        </div>
      </div>
    </div>
  );
}
