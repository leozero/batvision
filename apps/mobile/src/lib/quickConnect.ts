/** "ABC123" → "ABC 123": two halves read more easily from a couch (D111). */
export function spacedCode(code: string): string {
  return code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code;
}

/** The web's account page with the code filled in: where the TV's QR code leads (D111). */
export function approveUrl(server: string, code: string): string {
  return `${server}/account?code=${encodeURIComponent(code)}`;
}

/** What someone typed ("abc 123") as the server expects it ("ABC123"), or null. */
export function typedCode(text: string): string | null {
  const code = text.replace(/\s/g, "").toUpperCase();
  return code.length === 6 ? code : null;
}
