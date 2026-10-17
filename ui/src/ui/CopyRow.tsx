import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { copyText } from './copyText';
import { Check, Copy } from './Icon';

/** A recessed row holding `text` (selectable, in mono) and a Copy key that swaps to a check and `copied` for 2 s. */
export function CopyRow({ text, label, copy, copied }: { text: string; label: string; copy: string; copied: string }) {
  const { t } = useTranslation();
  const [done, setDone] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const run = () =>
    void copyText(text).then((ok) => {
      if (!ok) return;
      setDone(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setDone(false), 2000);
    });
  return (
    <div className="well copy-row">
      <span className="sr-only">{label}</span>
      <span className="copy-row-text">{text}</span>
      <Button variant="ghost" icon={done ? <Check /> : <Copy />} aria-label={copy} onClick={run}>
        {done ? t('common.copied') : t('common.copy')}
      </Button>
      <span className="sr-only" aria-live="polite">
        {done ? copied : ''}
      </span>
    </div>
  );
}
