import createClient from "openapi-fetch";
import type { components, paths } from "./schema";

export type { components, paths };
export type Schemas = components["schemas"];

/**
 * Typed client for the batvision server API.
 * `baseUrl` is empty for the web app (same origin) and the server URL for desktop/mobile.
 */
export function createApiClient(baseUrl = "", fetchImpl?: typeof fetch) {
  return createClient<paths>({ baseUrl, fetch: fetchImpl });
}

export type ApiClient = ReturnType<typeof createApiClient>;
