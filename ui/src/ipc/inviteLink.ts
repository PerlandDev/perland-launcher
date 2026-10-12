// The TypeScript mirror of launcher-core's `InviteLink::parse` and `canonical()`
// (crates/launcher-core/src/invite/link.rs). Both run the same generated table,
// ui/src/ipc/contract/invite_links.json, so they can't drift apart.
const TOKEN = /^[A-Za-z0-9_-]{43}$/;
/** Longest input looked at, in UTF-8 bytes, as in Rust's `raw.len()`. */
const MAX_INPUT = 512;

// Rust's `str::trim` and `char::is_whitespace` use Unicode White_Space, which differs from
// JavaScript's `\s` and `trim()` (U+FEFF is trimmed there, U+0085 is not).
const TRIM = /^\p{White_Space}+|\p{White_Space}+$/gu;
// `char::is_whitespace`, `char::is_control` (Cc: U+0000-U+001F, U+007F-U+009F) and `? # @ \`.
// eslint-disable-next-line no-control-regex
const REFUSED = /[\p{White_Space}\u0000-\u001f\u007f-\u009f?#@\\]/u;

const encoder = new TextEncoder();

function isLoopback(hostname: string): boolean {
  const host = hostname.toLowerCase();
  return host === 'localhost' || host === '[::1]' || /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host);
}

/** The canonical invite (lowercase host, no trailing slash), or null when `input` isn't one. */
export function inviteCanonical(input: string): string | null {
  const raw = input.replace(TRIM, '');
  // Checked on the raw text, as in Rust: URL parsing hides an empty `?` or `#` and turns `\` into `/`.
  if (!raw || encoder.encode(raw).length > MAX_INPUT || REFUSED.test(raw)) return null;
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return null;
  }
  const allowed = url.protocol === 'https:' || (url.protocol === 'http:' && isLoopback(url.hostname));
  if (!allowed || url.username || url.password || !url.hostname) return null;
  const token = url.pathname.replace(/^\//, '').replace(/\/$/, '');
  if (!TOKEN.test(token)) return null;
  return `${url.protocol}//${url.host}/${token}`;
}

export const isInviteLink = (input: string): boolean => inviteCanonical(input) !== null;

/** The invite's host, for "From play.example.com". Never show the token itself. */
export function inviteHost(invite: string): string | null {
  const canonical = inviteCanonical(invite);
  return canonical ? new URL(canonical).hostname : null;
}
