import { type ApiClient, createApiClient, type Schemas } from "@batvision/api-client";
import { session } from "./session";

export type { Schemas };

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

/** The server can't be reached: the app switches to the offline library. */
export class Unreachable extends Error {}

/** Adds the device token, and turns network failures into `Unreachable`. */
const authorizedFetch: typeof fetch = async (input, init) => {
  const { token } = session();
  const request = new Request(input, init);
  if (token) request.headers.set("Authorization", `Bearer ${token}`);
  try {
    const response = await fetch(request);
    // Behind a reverse proxy, a stopped server answers 502–504.
    if (response.status >= 502 && response.status <= 504)
      throw new Unreachable(response.statusText);
    return response;
  } catch (e) {
    if (e instanceof Unreachable) throw e;
    throw new Unreachable(e instanceof Error ? e.message : String(e));
  }
};

let client: { server: string; api: ApiClient } | null = null;

/** Client for the current server (rebuilt when the address changes). */
export function api(): ApiClient {
  const { server } = session();
  if (!server) throw new Unreachable("no server");
  if (client?.server !== server) client = { server, api: createApiClient(server, authorizedFetch) };
  return client.api;
}

type Result<T> = { data?: T; error?: unknown; response: Response };

/** Unwraps an openapi-fetch result, throwing an `ApiError` on failure. */
export async function call<T>(promise: Promise<Result<T>>): Promise<T> {
  const { data, error, response } = await promise;
  if (response.ok) return data as T;
  const body = (error ?? {}) as { code?: string; message?: string };
  throw new ApiError(response.status, body.code ?? "unknown", body.message ?? response.statusText);
}

/** A TMDB image, served (and cached) by the server without authentication. */
export function imageUrl(
  path: string | null | undefined,
  size: "w185" | "w342" | "w500" | "w780" | "w1280" | "original",
) {
  const { server } = session();
  return path && server ? `${server}/api/images/${size}${path}` : undefined;
}
