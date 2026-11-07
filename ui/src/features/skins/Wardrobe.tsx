import { useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery } from '@tanstack/react-query';
import { commands } from '../../ipc/commands';
import type { SkinEntry } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { Menu, type MenuHandle, type MenuItem } from '../../ui/Menu';
import { Plus, Upload } from '../../ui/Icon';

/** A skin's face, read straight off its texture: the head's front with the hat over it. */
function Face({ id, updated }: { id: string; updated: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const texture = useQuery({ queryKey: ['skinTexture', id, updated], queryFn: () => commands.skinTexture(id), staleTime: Infinity });
  useEffect(() => {
    const ctx = canvas.current?.getContext('2d');
    if (!ctx || !texture.data) return;
    const image = new Image();
    image.onload = () => {
      ctx.imageSmoothingEnabled = false;
      ctx.clearRect(0, 0, 8, 8);
      ctx.drawImage(image, 8, 8, 8, 8, 0, 0, 8, 8);
      ctx.drawImage(image, 40, 8, 8, 8, 0, 0, 8, 8);
    };
    image.src = texture.data;
  }, [texture.data]);
  return <canvas ref={canvas} width={8} height={8} className="wardrobe-face" aria-hidden />;
}

export function Wardrobe({
  entries,
  selected,
  wearer,
  onSelect,
  emptyNote,
  newItems,
  onImport,
  menuFor,
}: {
  entries: SkinEntry[];
  selected: string | null;
  /** The account whose worn skin gets the lamp. */
  wearer: string | null;
  onSelect: (id: string) => void;
  /** What an empty wardrobe says. */
  emptyNote: string;
  newItems: MenuItem[];
  onImport: () => void;
  menuFor: (entry: SkinEntry) => MenuItem[];
}) {
  const { t } = useTranslation();
  const newMenu = useRef<MenuHandle>(null);
  return (
    <aside className="wardrobe" aria-labelledby="wardrobe-title">
      <div className="flex items-center justify-between gap-2">
        <h2 id="wardrobe-title" className="font-bold">
          {t('skins.wardrobe')}
        </h2>
        <span className="text-sm text-ink-dim tabular">{entries.length}</span>
      </div>
      <div className="grid grid-cols-2 gap-2">
        <Button icon={<Plus />} aria-haspopup="menu" onClick={(e) => newMenu.current?.openAt(e.currentTarget)}>
          {t('skins.new')}
        </Button>
        <Button icon={<Upload />} onClick={onImport}>
          {t('skins.import')}
        </Button>
      </div>
      <Menu ref={newMenu} label={t('skins.new')} items={newItems} trigger={false} className="contents" />
      {entries.length === 0 ? (
        <p className="text-sm text-ink-dim">{emptyNote}</p>
      ) : (
        <ul className="wardrobe-list" aria-label={t('skins.wardrobe')}>
          {entries.map((entry) => {
            const isSelected = entry.id === selected;
            const worn = wearer !== null && entry.wornBy === wearer;
            return (
              <li key={entry.id} className="wardrobe-item" data-selected={isSelected || undefined}>
                <button
                  type="button"
                  className="wardrobe-pick"
                  aria-current={isSelected || undefined}
                  onClick={() => onSelect(entry.id)}
                >
                  <Face id={entry.id} updated={entry.updated} />
                  <span className="grid min-w-0 text-left">
                    <span className="truncate font-semibold">{entry.name}</span>
                    <span className="flex items-center gap-1.5 text-xs text-ink-dim">
                      {worn && <span className="lamp" aria-hidden />}
                      {worn ? t('skins.wearing') : t(`skins.model.${entry.model}`)}
                    </span>
                  </span>
                </button>
                <Menu label={t('skins.moreFor', { name: entry.name })} items={menuFor(entry)} compact floating />
              </li>
            );
          })}
        </ul>
      )}
    </aside>
  );
}
