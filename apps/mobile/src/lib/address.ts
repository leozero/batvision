/**
 * `media.example.com` → `https://media.example.com`; `null` when it can't be an address.
 * Parsed by hand: React Native's `URL` doesn't implement `host`.
 */
export function normalizeServer(input: string): string | null {
  const raw = input.trim();
  const withScheme = /^https?:\/\//i.test(raw) ? raw : `https://${raw}`;
  const match =
    /^(https?):\/\/([a-z0-9.-]+(?::\d{1,5})?|\[[0-9a-f:.]+\](?::\d{1,5})?)(?:[/?#].*)?$/i.exec(
      withScheme,
    );
  if (!match?.[1] || !match[2]) return null;
  return `${match[1].toLowerCase()}://${match[2].toLowerCase()}`;
}
