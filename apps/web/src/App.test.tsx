import "@testing-library/jest-dom/vitest";
import { createApiClient } from "@batvision/api-client";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

function fakeFetch(routes: Record<string, unknown>): typeof fetch {
  return async (input) => {
    const url = new URL(input instanceof Request ? input.url : String(input));
    const body = routes[url.pathname];
    return body === undefined ? new Response("not found", { status: 404 }) : Response.json(body);
  };
}

afterEach(cleanup);

describe("App", () => {
  it("shows server status and that setup is needed", async () => {
    const api = createApiClient(
      "http://test",
      fakeFetch({
        "/api/health": { status: "ok", version: "0.1.0" },
        "/api/setup": { needed: true },
      }),
    );
    render(<App api={api} />);
    expect(await screen.findByTestId("server-status")).toHaveTextContent("ok — v0.1.0");
    expect(screen.getByTestId("setup")).toHaveTextContent("First launch");
  });

  it("shows ready once an admin exists", async () => {
    const api = createApiClient(
      "http://test",
      fakeFetch({
        "/api/health": { status: "ok", version: "0.1.0" },
        "/api/setup": { needed: false },
      }),
    );
    render(<App api={api} />);
    expect(await screen.findByTestId("setup")).toHaveTextContent("Ready");
  });

  it("reports an unreachable server", async () => {
    const api = createApiClient("http://test", fakeFetch({}));
    render(<App api={api} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Server unreachable");
  });
});
