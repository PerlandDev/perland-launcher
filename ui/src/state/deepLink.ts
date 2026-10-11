import { commands } from '../ipc/commands';
import { navigate } from './route';
import { pushError } from './toasts';

/** Open New instance with the waiting `perland://` pack link, if there is one. Nothing is
 *  added until the player clicks Add pack. */
export async function takeDeepLink(): Promise<void> {
  try {
    const link = await commands.deepLinkTake();
    if (link) navigate({ name: 'newInstance', link });
  } catch (error) {
    pushError(error);
  }
}
