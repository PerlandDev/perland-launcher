import { useTranslation } from 'react-i18next';
import type { CreatableTeam } from '../../ipc/schema';

/** The Where value: `free` (the player's free server) or a team's id. */
export const FREE_WHERE = 'free';

/** Where a new server goes (spec 2026-10-21 §6.3), the create dialogs' first choice: the player's free server, while they can
 *  make it, then each team they own or are a member of, by name. The kind's segments, so the pick is a pressed key with the
 *  lime lamp. Nothing shows when the free server is the only place. */
export function WhereField({
  free,
  teams,
  value,
  onChange,
}: {
  free: boolean;
  teams: CreatableTeam[];
  value: string;
  onChange: (where: string) => void;
}) {
  const { t } = useTranslation();
  if (teams.length === 0) return null;
  const choices = [...(free ? [{ value: FREE_WHERE, label: t('teamServer.free') }] : []), ...teams.map((team) => ({ value: team.tenantId, label: team.name }))];
  return (
    <fieldset className="grid gap-1.5">
      <legend className="mb-1.5 text-sm font-semibold">{t('teamServer.where')}</legend>
      <div className="flex flex-wrap gap-2">
        {choices.map((choice) => (
          <label key={choice.value} className="seg min-w-0 text-ink" data-checked={value === choice.value ? '' : undefined}>
            <input type="radio" name="server-where" className="sr-only" checked={value === choice.value} onChange={() => onChange(choice.value)} />
            <span className="truncate">{choice.label}</span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}
