import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { describeError } from '../ipc/errors';
import { dismissToast, useToasts, type Toast } from '../state/toasts';
import { Button } from './Button';
import { Close, WarningDiamond } from './Icon';

function ToastItem({ toast }: { toast: Toast }) {
  const { t, i18n } = useTranslation();
  const text = describeError(i18n, toast.error);
  useEffect(() => {
    const timer = setTimeout(() => dismissToast(toast.id), 8000);
    return () => clearTimeout(timer);
  }, [toast.id]);
  return (
    <li className="panel flex items-start gap-3 p-4 shadow-[0_6px_0_var(--color-dirt-deep)]">
      <WarningDiamond className="mt-0.5 h-5 w-5 flex-none text-coral" aria-hidden />
      <div className="grid flex-1 gap-0.5">
        <p className="font-semibold">{text.message}</p>
        {text.fix && <p className="text-sm text-ink-dim">{text.fix}</p>}
      </div>
      <Button variant="ghost" size="icon" aria-label={t('common.dismiss')} icon={<Close />} onClick={() => dismissToast(toast.id)} />
    </li>
  );
}

export function Toasts() {
  const toasts = useToasts();
  return (
    <ol role="status" aria-live="polite" className="pointer-events-auto fixed right-4 bottom-4 z-40 grid w-[24rem] gap-3">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} />
      ))}
    </ol>
  );
}
