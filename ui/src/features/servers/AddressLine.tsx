import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { ServerAddress } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { copyText } from '../../ui/copyText';
import { Copy } from '../../ui/Icon';
import { addressText } from '../home/labels';

/** The address players type, with Copy. */
export function AddressLine({ address }: { address: ServerAddress | null }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  if (!address) return <p className="text-sm text-ink-dim">{t('server.noAddress')}</p>;
  const text = addressText(address);
  const copy = () =>
    void copyText(text).then(() => {
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 2000);
    });
  return (
    <p className="flex flex-wrap items-center gap-3">
      <span className="text-ink-dim">{t('server.address')}</span>
      <span className="selectable font-mono">{text}</span>
      <Button variant="ghost" icon={<Copy />} aria-label={t('server.copyAddress')} onClick={copy}>
        {copied ? t('common.copied') : t('common.copy')}
      </Button>
    </p>
  );
}
