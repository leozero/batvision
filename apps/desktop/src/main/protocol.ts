import { createReadStream, existsSync, statSync } from "node:fs";
import { extname, join, resolve, sep } from "node:path";
import { Readable } from "node:stream";
import { net, protocol } from "electron";
import type { Config } from "./config";
import type { Library } from "./library";
import { fetchWithTimeout } from "./server";

export const SCHEME = "app";
export const HOST = "batvision";
export const APP_URL = `${SCHEME}://${HOST}/`;

/** Must run before `app.ready`. */
export function registerScheme() {
  protocol.registerSchemesAsPrivileged([
    {
      scheme: SCHEME,
      privileges: {
        standard: true,
        secure: true,
        supportFetchAPI: true,
        corsEnabled: true,
        stream: true,
        codeCache: true,
      },
    },
  ]);
}

/** Responses that carry a session token, captured here so the page never sees it. */
const SIGN_IN =
  /^\/api\/(setup|auth\/login|auth\/login\/totp|auth\/invitations\/accept|auth\/quick-connect\/poll)$/;

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const MIME: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".webp": "image/webp",
  ".woff2": "font/woff2",
  ".json": "application/json",
  ".mp4": "video/mp4",
  ".vtt": "text/vtt; charset=utf-8",
};

/** A local file, with byte ranges so the video element can seek. */
export function serveFile(path: string, request: Request): Response {
  if (!existsSync(path) || !statSync(path).isFile())
    return new Response("not found", { status: 404 });
  const size = statSync(path).size;
  const type = MIME[extname(path).toLowerCase()] ?? "application/octet-stream";
  const range = /^bytes=(\d*)-(\d*)$/.exec(request.headers.get("range") ?? "");
  if (range && (range[1] || range[2])) {
    let start = range[1] ? Number(range[1]) : size - Number(range[2]);
    let end = range[1] && range[2] ? Number(range[2]) : size - 1;
    start = Math.max(0, start);
    end = Math.min(end, size - 1);
    if (start > end || start >= size) {
      return new Response(null, { status: 416, headers: { "content-range": `bytes */${size}` } });
    }
    const stream = Readable.toWeb(createReadStream(path, { start, end })) as ReadableStream;
    return new Response(stream, {
      status: 206,
      headers: {
        "content-type": type,
        "content-length": String(end - start + 1),
        "content-range": `bytes ${start}-${end}/${size}`,
        "accept-ranges": "bytes",
      },
    });
  }
  const stream = Readable.toWeb(createReadStream(path)) as ReadableStream;
  return new Response(stream, {
    headers: { "content-type": type, "content-length": String(size), "accept-ranges": "bytes" },
  });
}

/**
 * `app://batvision/` serves everything the page loads, from one origin:
 * - `/api/*` is forwarded to the server with the device token (the page never sees it);
 * - `/local/<item>/<file>` serves downloaded files;
 * - anything else is the bundled web interface (index.html for app routes).
 */
export function handleProtocol(opts: {
  webDir: string;
  config: Config;
  library: () => Library;
  onSignedIn: () => void;
  onSignedOut: () => void;
  onReachable: (reachable: boolean) => void;
}) {
  const { webDir, config } = opts;

  protocol.handle(SCHEME, async (request) => {
    const url = new URL(request.url);
    if (url.host !== HOST) return new Response("not found", { status: 404 });
    const path = decodeURIComponent(url.pathname);

    if (path.startsWith("/api/")) return proxy(request, url);

    if (path.startsWith("/local/")) {
      const [, , key, name] = path.split("/");
      try {
        const file = key && name ? opts.library().file(key, name) : null;
        return file ? serveFile(file, request) : new Response("not found", { status: 404 });
      } catch {
        return new Response("not found", { status: 404 });
      }
    }

    const file = resolve(webDir, `.${path}`);
    if (file.startsWith(webDir + sep) && extname(file) && existsSync(file)) {
      return net.fetch(`file://${file}`);
    }
    // An app route (`/movies/12`): the single-page app handles it.
    return serveFile(join(webDir, "index.html"), new Request(request.url));
  });

  async function proxy(request: Request, url: URL): Promise<Response> {
    const server = config.server;
    if (!server) return json(503, { code: "no_server", message: "no server configured" });

    const headers = new Headers(request.headers);
    for (const h of ["host", "origin", "referer", "cookie"]) headers.delete(h);
    const token = config.token;
    if (token) headers.set("authorization", `Bearer ${token}`);

    const hasBody = request.method !== "GET" && request.method !== "HEAD";
    let response: Response;
    try {
      response = await fetchWithTimeout(`${server}${url.pathname}${url.search}`, {
        method: request.method,
        headers,
        body: hasBody ? await request.arrayBuffer() : undefined,
        redirect: "manual",
      });
    } catch {
      opts.onReachable(false);
      return json(503, { code: "server_unreachable", message: "the server can't be reached" });
    }
    opts.onReachable(true);

    if (SIGN_IN.test(url.pathname) && response.ok) {
      const body = (await response.json()) as Record<string, unknown>;
      if (typeof body.token === "string") {
        config.token = body.token;
        delete body.token;
        opts.onSignedIn();
      }
      return json(response.status, body);
    }
    if (
      url.pathname === "/api/auth/logout" ||
      (url.pathname === "/api/me" && response.status === 401 && token)
    ) {
      if (config.token) {
        config.token = null;
        opts.onSignedOut();
      }
    }
    const out = new Headers(response.headers);
    out.delete("set-cookie");
    // Lengths and encodings no longer match once net.fetch has decoded the body.
    out.delete("content-encoding");
    if (!response.headers.get("content-range")) out.delete("content-length");
    const empty = response.status === 204 || response.status === 304 || request.method === "HEAD";
    return new Response(empty ? null : response.body, {
      status: response.status,
      statusText: response.statusText,
      headers: out,
    });
  }
}
