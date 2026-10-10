import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

/** A feature that exists in the plan but not yet in the launcher: visible, honest, inert. */
export function ComingSoon({ label, icon }: { label: string; icon?: ReactNode }) {
  const { t } = useTranslation();
  return (
    <div aria-disabled="true" className="panel flex items-center gap-3 px-4 py-3 text-ink-dim opacity-80">
      {icon}
      <span className="font-semibold">{label}</span>
      <span className="badge ml-auto">{t('common.comingSoon')}</span>
    </div>
  );
}
