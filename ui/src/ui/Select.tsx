import { useId } from 'react';

export interface Option {
  value: string;
  label: string;
}

export function Select({
  label,
  value,
  onChange,
  options,
  disabled,
  hint,
  error,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: Option[];
  disabled?: boolean;
  /** What the choice does, or why it can't be made, under it. */
  hint?: string;
  /** A refusal of this choice, under it (announced as it appears: it comes back from a request). */
  error?: string | null;
}) {
  const id = useId();
  const errorId = `${id}-error`;
  const hintId = `${id}-hint`;
  const describedBy = [hint && hintId, error && errorId].filter(Boolean).join(' ') || undefined;
  return (
    <div className="grid gap-1.5">
      <label htmlFor={id} className="text-sm font-semibold">
        {label}
      </label>
      <select
        id={id}
        className="field-input"
        value={value}
        disabled={disabled}
        aria-invalid={error ? true : undefined}
        aria-describedby={describedBy}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      {hint && (
        <p id={hintId} className="text-sm text-ink-dim">
          {hint}
        </p>
      )}
      {error && (
        <p id={errorId} role="alert" className="text-sm text-coral">
          {error}
        </p>
      )}
    </div>
  );
}
