import type { TFunction } from 'i18next';
import { pushNotice } from '../../state/toasts';

/** After a sign-in made because Play needed one: the Play that asked didn't happen, so say to press it again. */
export function pushSignedInNotice(t: TFunction) {
  pushNotice({ title: t('accounts.promptSignedIn') });
}
