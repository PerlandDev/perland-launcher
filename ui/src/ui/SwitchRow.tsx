import type { ReactNode } from 'react';
import { Toggle } from './Toggle';

/** A switch with its name and a line on what it does, the launcher's Settings row. */
export function SwitchRow({ label, hint, checked, disabled, onChange, after }: { label: string; hint: string; checked: boolean; disabled?: boolean; onChange: (next: boolean) => void; after?: ReactNode }) {
  return (
    <div className="grid gap-2">
      <div className="flex items-start justify-between gap-6">
        <div>
          <p className="font-semibold" aria-hidden>
            {label}
          </p>
          <p className="text-sm text-ink-dim">{hint}</p>
        </div>
        <Toggle checked={checked} label={label} disabled={disabled} onChange={onChange} />
      </div>
      {after}
    </div>
  );
}
