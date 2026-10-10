import { useCallback, useRef } from 'react';

/** Lets one run of an action be in flight at a time; the ref closes the gap before React re-renders. Returns whether it started. */
export function useSingleFlight() {
  const busy = useRef(false);
  return useCallback((start: (done: () => void) => void) => {
    if (busy.current) return false;
    busy.current = true;
    start(() => {
      busy.current = false;
    });
    return true;
  }, []);
}
