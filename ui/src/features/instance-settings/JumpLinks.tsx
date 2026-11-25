import { Fragment } from 'react';
import { useTranslation } from 'react-i18next';
import { reveal } from './reveal';

export interface Jump {
  key: string;
  label: string;
  /** Where the link lands, as a selector: a section's heading, or the control it is about. */
  target: string;
  block?: ScrollLogicalPosition;
}

/** A quiet row of in-page links to the parts of a long settings page. Flat text, never lit. */
export function JumpLinks({ jumps }: { jumps: Jump[] }) {
  const { t } = useTranslation();
  return (
    <nav aria-label={t('instanceSettings.jumpLabel')} className="-mb-4">
      <ul className="flex flex-wrap items-baseline gap-x-2 gap-y-1 text-sm">
        {jumps.map((jump, index) => (
          <Fragment key={jump.key}>
            {index > 0 && (
              <li aria-hidden className="text-ink-dim">
                ·
              </li>
            )}
            <li>
              <button
                type="button"
                className="jump-link"
                // Looked up within the page the links belong to, so a target is whatever is there at the click.
                onClick={(event) => reveal(event.currentTarget.closest('nav')?.parentElement?.querySelector(jump.target) ?? null, jump.block)}
              >
                {jump.label}
              </button>
            </li>
          </Fragment>
        ))}
      </ul>
    </nav>
  );
}
