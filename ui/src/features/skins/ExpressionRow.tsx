// The Create tab's ready-made faces: one small key per catalog expression, each showing the player's own face with
// that expression's eyes, brows and mouth on, in the colours they already have. A click puts the three on as one step.
// The faces are drawn only while the row is on screen (the Face category), and drawing one never touches the skin.
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { CatalogExpression, CatalogPiece, SkinLook } from '../../ipc/schema';
import type { Lang } from './catalogSearch';
import { FACE_SIZE } from './face';
import { applyExpression, expressionPieces, wearsExpression } from './look';
import type { LookEditor } from './useLook';

function ExpressionFace({ expression, pieces, look, editor }: { expression: CatalogExpression; pieces: readonly CatalogPiece[]; look: SkinLook | null; editor: LookEditor }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const preview = useRef(editor.previewFace);
  useEffect(() => {
    preview.current = editor.previewFace;
  });
  const { rev } = editor;
  // Drawn again whenever the skin changes (`look` for its pieces and colours, `rev` for its texels).
  useEffect(() => {
    let live = true;
    void preview
      .current((l) => applyExpression(l, expression, pieces))
      .then((face) => {
        const ctx = live && face ? canvas.current?.getContext('2d') : null;
        if (!ctx || !face) return;
        const image = ctx.createImageData(FACE_SIZE, FACE_SIZE);
        image.data.set(face);
        ctx.putImageData(image, 0, 0);
      })
      // A piece that can't be loaded now leaves the face as it was; the chip still works.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [expression, pieces, look, rev]);
  return <canvas ref={canvas} width={FACE_SIZE} height={FACE_SIZE} className="expression-face" aria-hidden />;
}

export function ExpressionRow({
  expressions,
  pieces,
  look,
  editor,
  lang,
}: {
  expressions: readonly CatalogExpression[];
  pieces: readonly CatalogPiece[];
  look: SkinLook | null;
  editor: LookEditor;
  lang: Lang;
}) {
  const { t } = useTranslation();
  // The key under the pointer or the keyboard: the heading reads its name.
  const [pointed, setPointed] = useState<string | null>(null);
  const offered = useMemo(() => expressions.filter((e) => expressionPieces(e, pieces) !== null), [expressions, pieces]);
  if (offered.length === 0) return null;
  const named = offered.find((e) => e.id === pointed) ?? offered.find((e) => wearsExpression(look, e));
  const label = t('skins.create.expression');
  return (
    <div className="create-group">
      <h3 className="create-heading">
        {label} · {named ? named.name[lang] : <span className="tabular">{t('skins.create.count', { count: offered.length })}</span>}
      </h3>
      <div role="group" aria-label={label} className="expression-row">
        {offered.map((e) => (
          <button
            key={e.id}
            type="button"
            className="expression-chip"
            aria-label={e.name[lang]}
            aria-current={wearsExpression(look, e) || undefined}
            title={e.name[lang]}
            onClick={() => editor.applyExpression(e)}
            onPointerEnter={() => setPointed(e.id)}
            onPointerLeave={() => setPointed((p) => (p === e.id ? null : p))}
            onFocus={() => setPointed(e.id)}
            onBlur={() => setPointed((p) => (p === e.id ? null : p))}
          >
            <ExpressionFace expression={e} pieces={pieces} look={look} editor={editor} />
          </button>
        ))}
      </div>
    </div>
  );
}
