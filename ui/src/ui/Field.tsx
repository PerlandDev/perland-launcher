import { cloneElement, useId, type ReactElement } from 'react';

type InputProps = { id?: string; 'aria-describedby'?: string; 'aria-invalid'?: boolean };

/** A label above, the control, then a hint or an error. The child input gets the ids it needs. */
export function Field({
  label,
  hint,
  error,
  children,
}: {
  label: string;
  hint?: string | null;
  error?: string | null;
  children: ReactElement<InputProps>;
}) {
  const id = useId();
  const noteId = `${id}-note`;
  const note = error ?? hint;
  return (
    <div className="grid gap-1.5">
      <label htmlFor={id} className="text-sm font-semibold">
        {label}
      </label>
      {cloneElement(children, {
        id,
        'aria-describedby': note ? noteId : undefined,
        'aria-invalid': error ? true : undefined,
      })}
      {note && (
        <p id={noteId} className={error ? 'text-sm text-coral' : 'text-sm text-ink-dim'}>
          {note}
        </p>
      )}
    </div>
  );
}
