import { useEffect, useId, useImperativeHandle, useRef, useState, type KeyboardEvent, type PointerEvent, type Ref } from 'react';
import { useTranslation } from 'react-i18next';
import { pixelUpscale } from '../../art/upscale';
import type { Focus } from '../../ipc/schema';
import { Button } from '../../ui/Button';

/** Where a picture background sits when the player hasn't chosen: the bottom middle (`.marquee-banner`). */
const DEFAULT_FOCUS: Focus = { x: 50, y: 100 };
/** One arrow key press, in percent. */
const STEP = 5;
/** A key press saves this long after the last one, so a run of presses saves once. */
const SAVE_DELAY_MS = 300;
/** The frame's shape; until the picture has loaded, it is assumed to be the same. */
const FRAME_ASPECT = 16 / 9;
const ARROWS: Partial<Record<string, [number, number]>> = {
  ArrowLeft: [-STEP, 0],
  ArrowRight: [STEP, 0],
  ArrowUp: [0, -STEP],
  ArrowDown: [0, STEP],
};

const clamp = (n: number) => Math.min(100, Math.max(0, Math.round(n)));

/** What the dialog can ask of a position still waiting to be saved. */
export interface FocusPickerHandle {
  /** Save it now, before another picture save. */
  flush: () => void;
  /** Drop it: a new background starts from the default anyway. */
  cancel: () => void;
}

/** The picture's box inside the frame, in percent of the frame, where `object-fit: contain` puts it. */
function contained(aspect: number) {
  if (aspect >= FRAME_ASPECT) {
    const height = (FRAME_ASPECT / aspect) * 100;
    return { left: 0, top: (100 - height) / 2, width: 100, height };
  }
  const width = (aspect / FRAME_ASPECT) * 100;
  return { left: (100 - width) / 2, top: 0, width, height: 100 };
}

/** "Change image…", Position: which part of a picture background stays in view when the marquee crops it.
 *  The whole picture sits in a recessed frame with a crosshair on the chosen point; a press or a drag moves
 *  it, and two visually hidden range inputs carry it for the keyboard and screen readers (the arrow keys move
 *  it by 5 the way they point). The marquee follows at once through `onPreview`; `onSave` runs when the
 *  pointer lets go, 300 ms after the last key press, or at once for Center, which goes back to the default
 *  (null), not to the middle. */
export function FocusPicker({
  picture,
  focus,
  disabled,
  onPreview,
  onSave,
  ref,
}: {
  /** The background picture (a data: URL). */
  picture: string;
  /** The saved position; null is the default. */
  focus: Focus | null;
  /** Another picture save is on its way. */
  disabled: boolean;
  onPreview: (focus: Focus | null) => void;
  onSave: (focus: Focus | null) => void;
  ref?: Ref<FocusPickerHandle>;
}) {
  const { t } = useTranslation();
  const id = useId();
  const [draft, setDraft] = useState<Focus | null>(focus);
  const [aspect, setAspect] = useState(FRAME_ASPECT);
  const [upscaled, setUpscaled] = useState(false);
  const shown = draft ?? DEFAULT_FOCUS;
  const box = contained(aspect);
  const surface = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // The position a key press is still holding back (undefined: nothing waits).
  const waiting = useRef<Focus | null | undefined>(undefined);
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
  // Closing the dialog right after a key press still keeps the position.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => () => flush(), []);

  const move = (next: Focus | null, when: 'now' | 'later' | 'drag') => {
    setDraft(next);
    onPreview(next);
    if (when === 'now') {
      cancel();
      save.current(next);
    } else if (when === 'later') {
      cancel();
      waiting.current = next;
      timer.current = setTimeout(flush, SAVE_DELAY_MS);
    }
  };

  /** The pointer's place on the picture; past an edge it stays on the edge. */
  const at = (event: PointerEvent<HTMLElement>): Focus | null => {
    const rect = surface.current?.getBoundingClientRect();
    if (!rect || rect.width <= 0 || rect.height <= 0) return null;
    return { x: clamp(((event.clientX - rect.left) / rect.width) * 100), y: clamp(((event.clientY - rect.top) / rect.height) * 100) };
  };
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (disabled || event.button !== 0) return;
    const next = at(event);
    if (!next) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    dragging.current = true;
    move(next, 'drag');
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    const next = at(event);
    if (next) move(next, 'drag');
  };
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    dragging.current = false;
    move(at(event) ?? shown, 'now');
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const step = ARROWS[event.key];
    if (!step) return;
    // Either input moves the crosshair the way the arrow points: up is towards the top of the picture.
    event.preventDefault();
    if (disabled) return;
    move({ x: clamp(shown.x + step[0]), y: clamp(shown.y + step[1]) }, 'later');
  };
  const range = (axis: 'x' | 'y') => (
    <input
      type="range"
      className="sr-only"
      min={0}
      max={100}
      step={STEP}
      value={shown[axis]}
      aria-label={t(axis === 'x' ? 'image.positionX' : 'image.positionY')}
      // A drag lands on any whole percent; the value itself snaps to the step.
      aria-valuetext={`${shown[axis]}%`}
      disabled={disabled}
      onKeyDown={onKeyDown}
      // Home, End, Page Up and Down, and a screen reader's own steps arrive as a change.
      onChange={(event) => move({ ...shown, [axis]: clamp(Number(event.currentTarget.value)) }, 'later')}
    />
  );

  return (
    <div role="group" aria-labelledby={`${id}-label`} aria-describedby={`${id}-hint`} className="focus-picker">
      <h3 id={`${id}-label`} className="art-section-label">
        {t('image.position')}
      </h3>
      <div className="focus-row">
        {/* The whole frame takes the pointer, so a press beside a narrow picture lands on its nearest edge. */}
        <div
          className="focus-frame"
          aria-hidden="true"
          data-disabled={disabled || undefined}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
        >
          <div className="focus-stage">
            <div ref={surface} className="focus-picture" style={{ left: `${box.left}%`, top: `${box.top}%`, width: `${box.width}%`, height: `${box.height}%` }}>
              <img
                src={picture}
                alt=""
                draggable={false}
                data-upscale={upscaled || undefined}
                onLoad={(e) => {
                  const { naturalWidth: width, naturalHeight: height } = e.currentTarget;
                  if (width > 0 && height > 0) setAspect(width / height);
                  const frame = surface.current;
                  setUpscaled(Boolean(frame && pixelUpscale({ width, height }, { width: frame.clientWidth, height: frame.clientHeight })));
                }}
              />
              <span className="focus-line focus-line-across" style={{ top: `${shown.y}%` }} />
              <span className="focus-line focus-line-down" style={{ left: `${shown.x}%` }} />
              <span className="focus-mark" style={{ left: `${shown.x}%`, top: `${shown.y}%` }} />
            </div>
          </div>
        </div>
        <div className="focus-side">
          <p id={`${id}-hint`} className="art-key-hint">
            {t('image.positionHint')}
          </p>
          <Button
            variant="ghost"
            aria-disabled={draft === null || disabled || undefined}
            onClick={() => {
              if (draft !== null && !disabled) move(null, 'now');
            }}
          >
            {t('image.center')}
          </Button>
        </div>
        {range('x')}
        {range('y')}
      </div>
    </div>
  );
}
