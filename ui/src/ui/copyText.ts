import { pushError } from '../state/toasts';

/** Copy `text` to the clipboard. Resolves true once copied; a clipboard that refuses becomes an error toast and
 *  resolves false. Never rejects. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch (error) {
    pushError(error);
    return false;
  }
}
