import { motdRuns } from './motd';

/** A server's MOTD in its § colours. Text only: nothing in it is ever parsed as markup. */
export function Motd({ text }: { text: string }) {
  return (
    <p className="motd whitespace-pre-line">
      {motdRuns(text).map((run, i) => (
        <span
          key={i}
          className={run.color ? `mc-${run.color}` : undefined}
          style={{
            fontWeight: run.bold ? 700 : undefined,
            fontStyle: run.italic ? 'italic' : undefined,
            textDecoration: [run.underline && 'underline', run.strike && 'line-through'].filter(Boolean).join(' ') || undefined,
          }}
        >
          {run.text}
        </span>
      ))}
    </p>
  );
}
