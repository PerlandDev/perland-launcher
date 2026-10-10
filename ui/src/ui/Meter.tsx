export function Meter({
  value,
  label,
  segments = 20,
  decorative = false,
}: {
  value: number;
  label: string;
  segments?: number;
  /** Inside a button the button's name already says it; hide the meter from assistive tech. */
  decorative?: boolean;
}) {
  const clamped = Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0));
  const lit = Math.round(clamped * segments);
  const a11y = decorative
    ? { 'aria-hidden': true }
    : {
        role: 'progressbar',
        'aria-label': label,
        'aria-valuemin': 0,
        'aria-valuemax': 100,
        'aria-valuenow': Math.round(clamped * 100),
      };
  return (
    <div className="meter" {...a11y}>
      {Array.from({ length: segments }, (_, i) => (
        <span key={i} data-on={i < lit ? '' : undefined} />
      ))}
    </div>
  );
}
