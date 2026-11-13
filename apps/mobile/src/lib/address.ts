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

/** Loopback, private, link-local and carrier-grade NAT ranges: never routed over the internet. */
function privateIpv4(host: string): boolean {
  const parts = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(host);
  if (!parts) return false;
  const [a, b] = [Number(parts[1]), Number(parts[2])];
  return (
    a === 10 ||
    a === 127 ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) ||
    (a === 169 && b === 254) ||
    (a === 100 && b >= 64 && b <= 127)
  );
}

/** A host that stays on the local network (or this device). */
export function isPrivateHost(host: string): boolean {
  const name = host.toLowerCase().replace(/\.$/, "");
  if (name.startsWith("[")) {
    const ip = name.slice(1, -1);
    const mapped = /^::ffff:(\d+\.\d+\.\d+\.\d+)$/.exec(ip);
    if (mapped?.[1]) return privateIpv4(mapped[1]);
    // Loopback, link-local (fe80::/10) and unique local (fc00::/7).
    return ip === "::1" || /^fe[89ab][0-9a-f]:/.test(ip) || /^f[cd][0-9a-f]{2}:/.test(ip);
  }
  if (/^[\d.]+$/.test(name)) return privateIpv4(name);
  // A name without a dot (`nas`) only resolves on the local network.
  return (
    !name.includes(".") ||
    name === "localhost" ||
    name.endsWith(".localhost") ||
    name.endsWith(".local")
  );
}

/**
 * The typed address is plain `http://` to a host on the internet: the password and
 * the device token would travel unencrypted (Android allows cleartext to any host).
 */
export function isPublicCleartext(input: string): boolean {
  const origin = normalizeServer(input);
  if (!origin?.startsWith("http://")) return false;
  const host = origin.slice("http://".length).replace(/:\d+$/, "");
  return !isPrivateHost(host);
}
