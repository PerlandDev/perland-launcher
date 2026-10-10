import type { AppEvent } from '../ipc/schema';
import { openActivity } from './activity';
import { isCrash } from './exit';
import { runStore } from './runStore';
import { pushError } from './toasts';

/** Everything the UI does with an event from Rust. */
export function handleEvent(event: AppEvent) {
  runStore.apply(event);
  if (event.type === 'warning') pushError(event.error);
  if (event.type === 'run_state' && event.state === 'idle' && event.exit && (event.exit.error || isCrash(event.exit))) {
    openActivity(event.instance);
  }
}
