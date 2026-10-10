import { useEffect, useId, useRef, type ReactNode } from 'react';

/** A native modal <dialog>: focus is trapped, Escape closes, focus returns to the opener. */
export function Dialog({
  open,
  onClose,
  title,
  children,
  actions,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children?: ReactNode;
  actions: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

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
          <h2 id={titleId} className="text-lg font-bold">
            {title}
          </h2>
          {children && <div className="mt-2 text-ink-dim">{children}</div>}
          <div className="mt-6 flex justify-end gap-3">{actions}</div>
        </>
      )}
    </dialog>
  );
}
