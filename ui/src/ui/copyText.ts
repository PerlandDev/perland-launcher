import { pushError } from '../state/toasts';

/** Copy `text` to the clipboard. A clipboard that refuses becomes an error toast, never an unhandled rejection. */
export function copyText(text: string): Promise<void> {
  return navigator.clipboard.writeText(text).catch(pushError);
}
