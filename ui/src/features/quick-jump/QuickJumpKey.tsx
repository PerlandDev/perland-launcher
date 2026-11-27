import { useTranslation } from 'react-i18next';
import { Search } from '../../ui/Icon';
import { openQuickJump, shortcut } from './store';

/** The title bar's search key: opens Go to, and its tooltip teaches the shortcut. Flat title-bar chrome, never lit. */
export function QuickJumpKey() {
  const { t } = useTranslation();
  const keys = shortcut();
  const label = t('quickJump.open');
  return (
    <button
      type="button"
      className="tb-btn"
      aria-label={label}
      title={t('quickJump.openTitle', { label, keys: keys.shown })}
      aria-keyshortcuts={keys.aria}
      aria-haspopup="dialog"
      onClick={openQuickJump}
    >
      <Search aria-hidden />
    </button>
  );
}
