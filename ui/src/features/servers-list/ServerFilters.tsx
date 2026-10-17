import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import type { ListSort, ServerFacets } from '../../ipc/schema';
import type { Option } from '../../ui/Select';
import { categoryName, languageName, regionName } from './labels';
import type { ListFilters } from './session';

const NONE: ServerFacets = { categories: [], languages: [], minecraftVersions: [], regions: [] };
const SORTS: ListSort[] = ['players', 'newest', 'name'];

/** The sort and the filters: one wrapping row of small keys, the Browse category keys' size, so the list
 *  never sits under a form. Every value goes to the Engine as the query; the launcher filters nothing
 *  itself. The choices come from the facets, which describe the whole list. */
export function ServerFilters({
  filters,
  facets,
  onChange,
}: {
  filters: ListFilters;
  facets: ServerFacets | null;
  onChange: (patch: Partial<ListFilters>) => void;
}) {
  const { t } = useTranslation();
  const { categories, languages, minecraftVersions, regions } = facets ?? NONE;
  const any = { value: '', label: t('servers.any') };
  const modded = filters.modded === null ? '' : filters.modded ? 'with' : 'without';
  return (
    <div className="servers-filters">
      <FilterKey
        label={t('servers.sort')}
        value={filters.sort}
        onChange={(value) => onChange({ sort: value as ListSort })}
        options={SORTS.map((s) => ({ value: s, label: t(`servers.sorts.${s}`) }))}
        filter={false}
      />
      <FilterKey
        label={t('servers.modded')}
        value={modded}
        onChange={(value) => onChange({ modded: value === '' ? null : value === 'with' })}
        options={[any, { value: 'with', label: t('servers.withMods') }, { value: 'without', label: t('servers.withoutMods') }]}
      />
      <FilterKey
        label={t('servers.minecraft')}
        value={filters.minecraft ?? ''}
        onChange={(value) => onChange({ minecraft: value || null })}
        options={[any, ...withChosen(minecraftVersions, filters.minecraft).map((v) => ({ value: v, label: v }))]}
      />
      <FilterKey
        label={t('servers.categoryLabel')}
        value={filters.category ?? ''}
        onChange={(value) => onChange({ category: value || null })}
        options={[any, ...withChosen(categories, filters.category).map((c) => ({ value: c, label: categoryName(t, c) }))]}
      />
      <FilterKey
        label={t('servers.languageLabel')}
        value={filters.language ?? ''}
        onChange={(value) => onChange({ language: value || null })}
        options={[any, ...withChosen(languages, filters.language).map((l) => ({ value: l, label: languageName(t, l) }))]}
      />
      {/* One region (today's case) is no choice at all, so the filter only appears with two or more. */}
      {regions.length >= 2 && (
        <FilterKey
          label={t('servers.regionLabel')}
          value={filters.region ?? ''}
          onChange={(value) => onChange({ region: value || null })}
          options={[any, ...withChosen(regions, filters.region).map((r) => ({ value: r, label: regionName(t, r) }))]}
        />
      )}
      <button type="button" className="chip" aria-pressed={filters.hasRoom} onClick={() => onChange({ hasRoom: !filters.hasRoom })}>
        {t('servers.hasRoom')}
      </button>
    </div>
  );
}

/** A select set into a small key: its name in dim ink, then the choice. A filter set to anything but Any
 *  sinks into its slot with the lime lamp, like a pressed category key; the sort always has a value, so it never does. */
function FilterKey({
  label,
  value,
  onChange,
  options,
  filter = true,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: Option[];
  filter?: boolean;
}) {
  const id = useId();
  return (
    <div className="filter-key" data-set={filter && value !== '' ? '' : undefined}>
      <label htmlFor={id}>{label}</label>
      <select id={id} value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
}

/** The facet's values, keeping a value already chosen even if the facets no longer name it, so the select shows it. */
function withChosen(values: string[], chosen: string | null): string[] {
  return chosen && !values.includes(chosen) ? [...values, chosen] : values;
}
