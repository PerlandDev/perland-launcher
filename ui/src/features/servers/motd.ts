/** One stretch of a MOTD with one style. `color` is the § code (`0`–`9`, `a`–`f`), or null. */
export interface MotdRun {
  text: string;
  color: string | null;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
}

const COLORS = '0123456789abcdef';
const PLAIN = { color: null, bold: false, italic: false, underline: false, strike: false };

/** Split a MOTD on its § codes. A colour resets the formats, as in Minecraft. */
export function motdRuns(motd: string): MotdRun[] {
  const runs: MotdRun[] = [];
  let style: Omit<MotdRun, 'text'> = { ...PLAIN };
  let text = '';
  const chars = [...motd];
  for (let i = 0; i < chars.length; i++) {
    const ch = chars[i] ?? '';
    if (ch !== '§') {
      text += ch;
      continue;
    }
    const code = (chars[i + 1] ?? '').toLowerCase();
    i++;
    if (text) runs.push({ text, ...style });
    text = '';
    if (COLORS.includes(code) && code !== '') style = { ...PLAIN, color: code };
    else if (code === 'l') style = { ...style, bold: true };
    else if (code === 'o') style = { ...style, italic: true };
    else if (code === 'n') style = { ...style, underline: true };
    else if (code === 'm') style = { ...style, strike: true };
    else if (code === 'r') style = { ...PLAIN };
    // `k` (obfuscated) and unknown codes are dropped.
  }
  if (text) runs.push({ text, ...style });
  return runs;
}

export const motdPlain = (motd: string): string =>
  motdRuns(motd)
    .map((r) => r.text)
    .join('');
