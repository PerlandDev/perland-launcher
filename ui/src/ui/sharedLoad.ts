/** One load shared by everyone who asks (a prefetch and the screen's `lazy`), so a chunk downloads once. A failed
 *  load is forgotten, so the next ask (Try again) loads it afresh instead of handing back the same failure. */
export function sharedLoad<T>(load: () => Promise<T>): () => Promise<T> {
  let pending: Promise<T> | null = null;
  return () => {
    pending ??= load().catch((error: unknown) => {
      pending = null;
      throw error;
    });
    return pending;
  };
}
