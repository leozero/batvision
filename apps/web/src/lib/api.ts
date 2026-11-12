import { type ApiClient, createApiClient, type Schemas } from "@batvision/api-client";
import { desktopInfo } from "./desktop";

export type { Schemas };

/** Same-origin client: the session cookie is sent automatically. */
export const api: ApiClient = createApiClient();

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public retryAfter?: number,
  ) {
    super(message);
  }
}

type Result<T> = { data?: T; error?: unknown; response: Response };

/** Unwraps an openapi-fetch result, throwing an `ApiError` on failure. */
export async function call<T>(promise: Promise<Result<T>>): Promise<T> {
  const { data, error, response } = await promise;
  if (response.ok) return data as T;
  const body = (error ?? {}) as { code?: string; message?: string };
  const retry = Number(response.headers.get("retry-after")) || undefined;
  throw new ApiError(
    response.status,
    body.code ?? "unknown",
    body.message ?? response.statusText,
    retry,
  );
}

/** URL of a TMDB image served (and cached) by the server. */
export function imageUrl(
  path: string | null | undefined,
  size: "w185" | "w342" | "w500" | "w780" | "w1280" | "original",
) {
  return path ? `/api/images/${size}${path}` : undefined;
}

export type DeviceInfo = Schemas["DeviceInfo"];

/** "Firefox · macOS": shown in the device list. */
export function thisDevice(): DeviceInfo {
  // The desktop app signs in as a desktop device: it gets a token, not a cookie.
  const desktop = desktopInfo();
  if (desktop) return desktop.device;
  const ua = navigator.userAgent;
  const browser = /Edg\//.test(ua)
    ? "Edge"
    : /Firefox\//.test(ua)
      ? "Firefox"
      : /Chrome\//.test(ua)
        ? "Chrome"
        : /Safari\//.test(ua)
          ? "Safari"
          : "Browser";
  const os = /Windows/.test(ua)
    ? "Windows"
    : /Android/.test(ua)
      ? "Android"
      : /iPhone|iPad/.test(ua)
        ? "iOS"
        : /Mac OS X/.test(ua)
          ? "macOS"
          : /Linux/.test(ua)
            ? "Linux"
            : "";
  return { name: os ? `${browser} · ${os}` : browser, platform: "web" };
}
