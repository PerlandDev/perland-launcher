import { describe, expect, it } from 'vitest';
import cases from './contract/invite_links.json';
import { inviteCanonical, inviteHost, isInviteLink } from './inviteLink';

describe('invite links', () => {
  it.each(cases.map((c) => [c.input, c.canonical] as const))('%j → %j, exactly as Rust decides', (input, canonical) => {
    expect(inviteCanonical(input)).toBe(canonical);
    expect(isInviteLink(input)).toBe(canonical !== null);
  });

  it('names only the host', () => {
    expect(inviteHost(`https://play.example.com/${'A'.repeat(43)}`)).toBe('play.example.com');
    expect(inviteHost('nope')).toBeNull();
  });

  it('trims like Rust: U+FEFF is not whitespace, U+0085 and U+00A0 are', () => {
    const link = `https://play.example.com/${'A'.repeat(43)}`;
    const [nel, nbsp, bom] = [0x85, 0xa0, 0xfeff].map((code) => String.fromCodePoint(code));
    expect(inviteCanonical(`${nel}${link}${nbsp}`)).toBe(link);
    expect(inviteCanonical(`${bom}${link}`)).toBeNull();
  });
});
