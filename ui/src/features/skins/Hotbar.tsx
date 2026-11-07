import { useTranslation } from 'react-i18next';
import { Brush, Eraser, FlipHorizontal2, Pencil, Pipette, Redo, Sparkles, Sun, Undo } from 'pixelarticons/react';
import type { ReactNode } from 'react';
import type { Tool } from './paint';
import { SLOT_TOOLS } from './tools';

const TOOL_ICONS: Record<Tool, ReactNode> = {
  pencil: <Pencil aria-hidden />,
  eraser: <Eraser aria-hidden />,
  fill: <Brush aria-hidden />,
  picker: <Pipette aria-hidden />,
  shade: <Sun aria-hidden />,
  noise: <Sparkles aria-hidden />,
};


export function Hotbar({
  tool,
  onTool,
  mirror,
  onMirror,
  canUndo,
  canRedo,
  onUndo,
  onRedo,
}: {
  tool: Tool;
  onTool: (tool: Tool) => void;
  mirror: boolean;
  onMirror: () => void;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div role="toolbar" aria-label={t('skins.hotbar')} className="hotbar">
      <div role="radiogroup" aria-label={t('skins.tools')} className="hotbar-group">
        {SLOT_TOOLS.map((each, i) => (
          <button
            key={each}
            type="button"
            role="radio"
            aria-checked={tool === each}
            className="hotbar-slot"
            title={`${t(`skins.tool.${each}`)} (${i + 1})`}
            onClick={() => onTool(each)}
          >
            <span className="hotbar-key tabular" aria-hidden>
              {i + 1}
            </span>
            {TOOL_ICONS[each]}
            <span className="hotbar-label">{t(`skins.tool.${each}`)}</span>
          </button>
        ))}
      </div>
      <span className="hotbar-seam" aria-hidden />
      <button type="button" aria-pressed={mirror} className="hotbar-slot" title={`${t('skins.mirror')} (7)`} onClick={onMirror}>
        <span className="hotbar-key tabular" aria-hidden>
          7
        </span>
        <FlipHorizontal2 aria-hidden />
        <span className="hotbar-label">{t('skins.mirror')}</span>
      </button>
      <button type="button" className="hotbar-slot" disabled={!canUndo} title={`${t('skins.undo')} (8 · Ctrl+Z)`} onClick={onUndo}>
        <span className="hotbar-key tabular" aria-hidden>
          8
        </span>
        <Undo aria-hidden />
        <span className="hotbar-label">{t('skins.undo')}</span>
      </button>
      <button type="button" className="hotbar-slot" disabled={!canRedo} title={`${t('skins.redo')} (9 · Ctrl+Y)`} onClick={onRedo}>
        <span className="hotbar-key tabular" aria-hidden>
          9
        </span>
        <Redo aria-hidden />
        <span className="hotbar-label">{t('skins.redo')}</span>
      </button>
    </div>
  );
}
