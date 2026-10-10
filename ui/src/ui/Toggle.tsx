/**
 * `disabled` is aria-disabled, not the attribute: Chromium blurs a focused button the moment it
 * becomes natively disabled, which would throw a keyboard player out of the list mid-toggle.
 */
export function Toggle({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      aria-disabled={disabled || undefined}
      className="toggle"
      onClick={() => {
        if (!disabled) onChange(!checked);
      }}
    >
      <span />
    </button>
  );
}
