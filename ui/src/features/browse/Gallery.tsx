import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { GalleryImage } from '../../ipc/schema';
import { useBrowseImage } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';

function Picture({ url, alt, large = false }: { url: string; alt: string; large?: boolean }) {
  const image = useBrowseImage(url, 'gallery');
  return image.data ? (
    <img src={image.data} alt={alt} className={large ? 'gallery-full' : 'gallery-thumb'} draggable={false} />
  ) : (
    <span className={`${large ? 'gallery-full' : 'gallery-thumb'} gallery-empty`} />
  );
}

export function Gallery({ images }: { images: GalleryImage[] }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState<number | null>(null);
  if (images.length === 0) return null;
  const name = (i: number) => images[i]?.title || t('project.galleryImage', { n: i + 1, total: images.length });
  const current = open === null ? null : images[open];
  return (
    <section aria-label={t('project.gallery')}>
      <ul className="gallery-strip">
        {images.map((image, i) => (
          <li key={image.url} className="flex-none">
            <button type="button" className="gallery-open" aria-label={name(i)} onClick={() => setOpen(i)}>
              <Picture url={image.url} alt="" />
            </button>
          </li>
        ))}
      </ul>
      <Dialog
        open={current !== null && current !== undefined}
        onClose={() => setOpen(null)}
        title={open === null ? '' : name(open)}
        actions={<Button onClick={() => setOpen(null)}>{t('common.close')}</Button>}
      >
        {current && <Picture url={current.url} alt={name(open!)} large />}
      </Dialog>
    </section>
  );
}
