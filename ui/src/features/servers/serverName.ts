/** The longest server name the launcher saves, in characters: the core's `MAX_NAME_CHARS`. */
export const MAX_NAME_CHARS = 64;

/** A server's name as the core saves it: trimmed, then cut to `MAX_NAME_CHARS` characters,
 *  counted as code points like Rust's `chars()`, so an emoji is one. */
export const savedServerName = (name: string) => Array.from(name.trim()).slice(0, MAX_NAME_CHARS).join('');
