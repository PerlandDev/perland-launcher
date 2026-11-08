import { useContext, useEffect, useImperativeHandle, useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent, type Ref, type WheelEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { bannerBox, clampZoom, covers, DEFAULT_FOCUS, dragFocus, MAX_ZOOM, MIN_ZOOM, ZOOM_STEP, type Size } from '../../art/bannerLayout';
import { Marquee } from '../../art/Marquee';
import { artSeed } from '../../art/seed';
import type { BannerEdges, BannerView, Focus, Instance } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { Minus, Plus } from '../../ui/Icon';
import { HeroPreview } from './heroPreview';

/** One arrow key press on the position, in percent. */
const STEP = 5;
/** A key press, a wheel step or a slider move saves this long after the last one, so a run of them saves once. */
const SAVE_DELAY_MS = 300;
/** Home's header when it can't be measured (a test, or no Home on screen). */
const FALLBACK_HERO: Size = { width: 1000, height: 562 };
const ARROWS: Partial<Record<string, [number, number]>> = {
  ArrowLeft: [-STEP, 0],
  ArrowRight: [STEP, 0],
  ArrowUp: [0, -STEP],
  ArrowDown: [0, STEP],
};

const clamp = (n: number) => Math.min(100, Math.max(0, Math.round(n)));

/** Each setting at its default is stored as null, so Reset knows when there is nothing to reset. */
function normal(view: BannerView): BannerView {
  return { focus: view.focus, zoom: view.zoom === 100 ? null : view.zoom, edges: view.edges === 'dark' ? null : view.edges };
}

/** What the dialog can ask of a view still waiting to be saved. */
export interface BannerAdjustHandle {
  /** Save it now. */
  flush: () => void;
  /** Drop it. */
  cancel: () => void;
}

/** "Adjust the background": a preview of Home's header (the picture under the dimmed name and Play), where a drag
 *  moves the picture and the wheel zooms it; under it the zoom, and the edges while the picture no longer covers
 *  the header; then Reset and Done. Every change shows at once (`onPreview`, so the marquee behind the backdrop
 *  follows too) and saves when a drag ends, 300 ms after the last key press, wheel step or slider move, and at
 *  once for Reset and the edges. Two visually hidden range inputs carry the position for the keyboard and screen
 *  readers. */
export function BannerAdjust({
  instance,
  picture,
  disabled,
  onPreview,
  onSave,
  onDone,
  ref,
}: {
  instance: Instance;
  /** The background picture (a data: URL). */
  picture: string;
  /** Another picture save is on its way. */
  disabled: boolean;
  onPreview: (view: BannerView) => void;
  onSave: (view: BannerView) => void;
  onDone: () => void;
  ref?: Ref<BannerAdjustHandle>;
}) {
  const { t } = useTranslation();
  const hero = useContext(HeroPreview);
  // The header as it is on screen now: the preview has its shape, and lays out at its size before scaling down.
  const [header] = useState<Size>(() => {
    const rect = document.querySelector('[data-home-hero]')?.getBoundingClientRect();
    return rect && rect.width > 0 && rect.height > 0 ? { width: rect.width, height: rect.height } : FALLBACK_HERO;
  });
  const [draft, setDraft] = useState<BannerView>(() => ({ focus: instance.bannerFocus, zoom: instance.bannerZoom, edges: instance.bannerEdges }));
  const [natural, setNatural] = useState<Size | null>(null);
  const [scale, setScale] = useState(0.5);
  const [dragging, setDragging] = useState(false);
  const stage = useRef<HTMLDivElement>(null);
  const focus = draft.focus ?? DEFAULT_FOCUS;
  const zoom = draft.zoom ?? 100;
  const edges: BannerEdges = draft.edges ?? 'dark';
  const atDefault = draft.focus === null && draft.zoom === null && draft.edges === null;

  // The view last shown, for a cancelled pointer: its event carries no real place.
  const lastShown = useRef(draft);
  const drag = useRef<{ x: number; y: number; from: Focus } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // The view a key press is still holding back (undefined: nothing waits).
  const waiting = useRef<BannerView | undefined>(undefined);
  const save = useRef(onSave);
  useEffect(() => {
    save.current = onSave;
  }, [onSave]);

  const cancel = () => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
    waiting.current = undefined;
  };
  const flush = () => {
    const next = waiting.current;
    cancel();
    if (next !== undefined) save.current(next);
  };
  useImperativeHandle(ref, () => ({ flush, cancel }));
  // Leaving the view (Back, Done, Escape) right after a change still keeps it.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => () => flush(), []);

  // The preview fills the well's width; Home's header is drawn at its own size and scaled to it.
  useEffect(() => {
    const el = stage.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(([entry]) => {
      const width = entry?.contentRect.width ?? 0;
      if (width > 0) setScale(width / header.width);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [header.width]);

  const move = (next: BannerView, when: 'now' | 'later' | 'drag') => {
    const view = normal(next);
    lastShown.current = view;
    setDraft(view);
    onPreview(view);
    if (when === 'now') {
      cancel();
      save.current(view);
    } else if (when === 'later') {
      cancel();
      waiting.current = view;
      timer.current = setTimeout(flush, SAVE_DELAY_MS);
    }
  };
  const zoomTo = (value: number) => {
    if (!disabled) move({ ...draft, zoom: clampZoom(value) }, 'later');
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (disabled || event.button !== 0 || !natural) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    drag.current = { x: event.clientX, y: event.clientY, from: focus };
    setDragging(true);
  };
  const dragged = (event: PointerEvent<HTMLDivElement>): BannerView | null => {
    const start = drag.current;
    const rect = event.currentTarget.getBoundingClientRect();
    if (!start || !natural || rect.width <= 0) return null;
    // Screen pixels to the header's own pixels.
    const k = rect.width / header.width;
    const delta = { width: (event.clientX - start.x) / k, height: (event.clientY - start.y) / k };
    return { ...draft, focus: dragFocus(start.from, delta, header, bannerBox(header, natural, draft.zoom, start.from)) };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const next = dragged(event);
    if (next) move(next, 'drag');
  };
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag.current) return;
    const next = dragged(event) ?? lastShown.current;
    drag.current = null;
    setDragging(false);
    move(next, 'now');
  };
  const onPointerCancel = () => {
    if (!drag.current) return;
    drag.current = null;
    setDragging(false);
    move(lastShown.current, 'now');
  };
  const onWheel = (event: WheelEvent<HTMLDivElement>) => {
    if (event.deltaY === 0) return;
    zoomTo(zoom + (event.deltaY < 0 ? ZOOM_STEP : -ZOOM_STEP));
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const step = ARROWS[event.key];
    if (!step) return;
    // Either input moves the picture's point the way the arrow points: up is towards the top of the picture.
    event.preventDefault();
    if (disabled) return;
    move({ ...draft, focus: { x: clamp(focus.x + step[0]), y: clamp(focus.y + step[1]) } }, 'later');
  };
  const range = (axis: 'x' | 'y') => (
    <input
      type="range"
      className="sr-only"
      min={0}
      max={100}
      step={STEP}
      value={focus[axis]}
      aria-label={t(axis === 'x' ? 'image.positionX' : 'image.positionY')}
      // A drag lands on any whole percent; the value itself snaps to the step.
      aria-valuetext={`${focus[axis]}%`}
      disabled={disabled}
      onKeyDown={onKeyDown}
      // Home, End, Page Up and Down, and a screen reader's own steps arrive as a change.
      onChange={(event) => move({ ...draft, focus: { ...focus, [axis]: clamp(Number(event.currentTarget.value)) } }, 'later')}
    />
  );
  const shown: Instance = { ...instance, bannerFocus: draft.focus, bannerZoom: draft.zoom, bannerEdges: draft.edges };
  const fill = `${((zoom - MIN_ZOOM) / (MAX_ZOOM - MIN_ZOOM)) * 100}%`;
  const edgeChoice = (kind: BannerEdges, label: string) => (
    <label className="seg seg-sm" data-checked={edges === kind ? '' : undefined}>
      <input
        type="radio"
        name={`banner-edges-${instance.id}`}
        className="sr-only"
        checked={edges === kind}
        disabled={disabled}
        onChange={() => move({ ...draft, edges: kind }, 'now')}
      />
      {label}
    </label>
  );

  return (
    <div className="banner-adjust">
      <div className="banner-well">
        <div className="banner-stage" style={{ aspectRatio: `${header.width} / ${header.height}` }}>
          <div ref={stage} className="banner-preview" aria-hidden="true" inert>
            <div className="banner-preview-scale" style={{ width: header.width, height: header.height, transform: `scale(${scale})` }}>
              {hero ? (
                hero(shown)
              ) : (
                <Marquee
                  seed={artSeed(instance)}
                  attract={false}
                  banner={picture}
                  focus={draft.focus}
                  zoom={draft.zoom}
                  edges={draft.edges}
                  className="absolute inset-0"
                />
              )}
            </div>
          </div>
          {/* Over the preview, the surface the pointer and the wheel use; the preview itself is inert. */}
          <div
            className="banner-preview-drag"
            aria-hidden="true"
            data-dragging={dragging || undefined}
            data-disabled={disabled || undefined}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerCancel}
            onWheel={onWheel}
          />
        </div>
        {/* The picture's own size, which a drag needs to know how far the picture can go. */}
        <img
          className="banner-probe"
          src={picture}
          alt=""
          aria-hidden="true"
          onLoad={(e) => {
            const { naturalWidth: width, naturalHeight: height } = e.currentTarget;
            if (width > 0 && height > 0) setNatural({ width, height });
          }}
        />
        {range('x')}
        {range('y')}
      </div>
      <div className="banner-controls">
        <label htmlFor={`banner-zoom-${instance.id}`} className="banner-control-label">
          {t('image.zoom')}
        </label>
        <div className="banner-zoom">
          <Button variant="ghost" size="icon" aria-label={t('image.zoomOut')} title={t('image.zoomOut')} icon={<Minus />} aria-disabled={disabled || zoom <= MIN_ZOOM || undefined} onClick={() => zoomTo(zoom - ZOOM_STEP)} />
          <input
            id={`banner-zoom-${instance.id}`}
            type="range"
            className="range"
            min={MIN_ZOOM}
            max={MAX_ZOOM}
            step={ZOOM_STEP}
            value={zoom}
            aria-valuetext={t('image.zoomValue', { value: zoom })}
            disabled={disabled}
            style={{ '--fill': fill } as CSSProperties}
            onChange={(event) => zoomTo(Number(event.currentTarget.value))}
          />
          <Button variant="ghost" size="icon" aria-label={t('image.zoomIn')} title={t('image.zoomIn')} icon={<Plus />} aria-disabled={disabled || zoom >= MAX_ZOOM || undefined} onClick={() => zoomTo(zoom + ZOOM_STEP)} />
          <span className="banner-zoom-value tabular" aria-hidden="true">
            {t('image.zoomValue', { value: zoom })}
          </span>
        </div>
        {/* Only a picture that no longer covers the header has edges; hidden, the row keeps its room. */}
        <div role="radiogroup" aria-labelledby={`banner-edges-label-${instance.id}`} className="banner-edges-row" style={covers(draft.zoom) ? { visibility: 'hidden' } : undefined}>
          <span id={`banner-edges-label-${instance.id}`} className="banner-control-label">
            {t('image.edges')}
          </span>
          <div className="flex gap-2">
            {edgeChoice('dark', t('image.edgesDark'))}
            {edgeChoice('pixel', t('image.edgesPixel'))}
          </div>
        </div>
      </div>
      <div className="mt-6 flex justify-between gap-3">
        <Button
          variant="ghost"
          aria-disabled={atDefault || disabled || undefined}
          onClick={() => {
            if (!atDefault && !disabled) move({ focus: null, zoom: null, edges: null }, 'now');
          }}
        >
          {t('image.reset')}
        </Button>
        <Button onClick={onDone}>{t('image.done')}</Button>
      </div>
    </div>
  );
}
