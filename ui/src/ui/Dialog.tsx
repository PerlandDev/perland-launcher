import { useEffect, useId, useRef, type ReactNode } from 'react';
import { Button } from './Button';
import { ChevronDown } from './Icon';

/** A native modal <dialog>: focus is trapped, Escape closes, focus returns to the opener. */
export function Dialog({
  open,
  onClose,
  title,
  children,
  actions,
  description,
  back,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  /** One line under the title that says what the dialog is for; it describes the dialog. */
  description?: string;
  /** A step this dialog came from: a back key leads the title, like a screen's. */
  back?: { label: string; onBack: () => void };
  children?: ReactNode;
  /** The keys under the body; none when the body carries its own (a form step with its buttons). */
  actions?: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-labelledby={titleId}
      aria-describedby={description ? descriptionId : undefined}
      onClose={() => {
        if (open) onClose();
      }}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      {open && (
        <>
          {back ? (
            // The key sits in the padding's edge, so its glyph lines up with the body's text, as on a screen.
            <div className="-ml-2 flex items-center gap-2">
              <Button variant="ghost" size="icon" aria-label={back.label} title={back.label} onClick={back.onBack} icon={<ChevronDown className="rotate-90" />} />
              <h2 id={titleId} className="text-lg font-bold">
                {title}
              </h2>
            </div>
          ) : (
            <h2 id={titleId} className="text-lg font-bold">
              {title}
            </h2>
          )}
          {description && (
            <p id={descriptionId} className="mt-2 text-ink-dim">
              {description}
            </p>
          )}
          {children && <div className="mt-2 text-ink-dim">{children}</div>}
          {actions != null && <div className="mt-6 flex justify-end gap-3">{actions}</div>}
        </>
      )}
    </dialog>
  );
}
