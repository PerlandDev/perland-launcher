import { describe, expect, it } from 'vitest';
import { addressText } from './labels';

describe('addressText', () => {
  it('writes host:port, with brackets around IPv6', () => {
    expect(addressText({ host: 'play.example.com', port: 25566 })).toBe('play.example.com:25566');
    expect(addressText({ host: '2001:db8::1', port: 25565 })).toBe('[2001:db8::1]:25565');
  });
});
