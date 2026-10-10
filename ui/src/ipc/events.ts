import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { appEvent, type AppEvent } from './schema';

export const APP_EVENT = 'launcher://event';
export const CLOSE_REQUESTED = 'launcher://close-requested';

/** Parse one event from Rust; anything that doesn't match the schema is dropped with a warning. */
export function decodeEvent(raw: unknown): AppEvent | null {
  const result = appEvent.safeParse(raw);
  if (!result.success) {
    console.warn('Ignoring an event the UI does not understand', result.error.issues);
    return null;
  }
  return result.data;
}

export function onAppEvent(handler: (event: AppEvent) => void): Promise<UnlistenFn> {
  return listen<unknown>(APP_EVENT, (message) => {
    const event = decodeEvent(message.payload);
    if (event) handler(event);
  });
}

export function onCloseRequested(handler: () => void): Promise<UnlistenFn> {
  return listen(CLOSE_REQUESTED, () => handler());
}
