import { net } from "electron";
import type { Config } from "./config";

/** The server could not be reached (as opposed to answering with an error). */
export class Unreachable extends Error {}

export class HttpError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

/** Normalizes what the user typed into an origin: `media.example.com` → `https://media.example.com`. */
export function normalizeServer(input: string): string | null {
  const trimmed = input.trim().replace(/\/+$/, "");
  if (!trimmed) return null;
  const withScheme = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  try {
    const url = new URL(withScheme);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    return url.origin;
  } catch {
    return null;
  }
}

/**
 * `fetch` that fails with `Unreachable` when the server doesn't answer in time, or
 * when a reverse proxy answers for it (502–504: the server is stopped or restarting).
 * The timeout covers the response headers only: bodies (videos) may take hours.
 */
export async function fetchWithTimeout(
  url: string,
  init: RequestInit & { duplex?: "half" } = {},
  timeoutMs = 15_000,
): Promise<Response> {
  const controller = new AbortController();
  const outer = init.signal;
  outer?.addEventListener("abort", () => controller.abort(outer.reason));
  const timer = setTimeout(() => controller.abort(new Unreachable("timeout")), timeoutMs);
  let response: Response;
  try {
    response = await net.fetch(url, { ...init, signal: controller.signal });
  } catch (e) {
    if (outer?.aborted) throw e;
    throw new Unreachable(e instanceof Error ? e.message : String(e));
  } finally {
    clearTimeout(timer);
  }
  if (answeredByProxy(response)) {
    void response.body?.cancel().catch(() => {});
    throw new Unreachable(`${response.status} ${response.statusText}`.trim());
  }
  return response;
}

/**
 * 502–504 come from the reverse proxy in front of a stopped server. The server's own
 * 502 (`upstream_error`: TMDB failed) is JSON like all its errors, and stays an error.
 */
function answeredByProxy(response: Response) {
  if (response.status < 502 || response.status > 504) return false;
  return !(response.headers.get("content-type") ?? "").includes("application/json");
}

/** A body that stops arriving (connection lost midway) is `Unreachable` too. */
export async function readBody<T>(read: () => Promise<T>, signal?: AbortSignal | null): Promise<T> {
  try {
    return await read();
  } catch (e) {
    if (signal?.aborted || e instanceof SyntaxError) throw e;
    throw new Unreachable(e instanceof Error ? e.message : String(e));
  }
}

/** JSON calls to the server with the device token, from the main process. */
export class ServerClient {
  constructor(
    private config: Config,
    private onSignedOut: () => void,
  ) {}

  get ready() {
    return Boolean(this.config.server && this.config.token);
  }

  url(path: string) {
    return `${this.config.server}${path}`;
  }

  async fetch(path: string, init: RequestInit = {}, timeoutMs?: number): Promise<Response> {
    const token = this.config.token;
    if (!this.config.server || !token) throw new HttpError(401, "signed out");
    const headers = new Headers(init.headers);
    headers.set("authorization", `Bearer ${token}`);
    const response = await fetchWithTimeout(this.url(path), { ...init, headers }, timeoutMs);
    if (response.status === 401) {
      // Revoked from another device.
      this.config.token = null;
      this.onSignedOut();
    }
    return response;
  }

  async json<T>(path: string, body?: unknown): Promise<T> {
    const response = await this.fetch(path, {
      method: body === undefined ? "GET" : "POST",
      headers: body === undefined ? undefined : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!response.ok) {
      const text = await response.text().catch(() => "");
      throw new HttpError(response.status, text || response.statusText);
    }
    if (response.status === 204) return undefined as T;
    return readBody(() => response.json() as Promise<T>);
  }
}
