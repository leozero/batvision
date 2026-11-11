import "@testing-library/jest-dom/vitest";
import { createApiClient } from "@batvision/api-client";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

function fakeFetch(routes: Record<string, unknown>): typeof fetch {
  return async (input) => {
    const url = new URL(input instanceof Request ? input.url : String(input));
    const body = routes[url.pathname];
    return body === undefined ? new Response("not found", { status: 404 }) : Response.json(body);
  };
}

describe("App", () => {
  it("shows server status and detected encoders", async () => {
    const api = createApiClient(
      "http://test",
      fakeFetch({
        "/api/health": { status: "ok", version: "0.1.0" },
        "/api/system/info": {
          version: "0.1.0",
          ffmpeg: { version: "ffmpeg version 7.1", working_encoders: ["hevc_nvenc", "av1_nvenc"] },
        },
      }),
    );
    render(<App api={api} />);
    expect(await screen.findByTestId("server-status")).toHaveTextContent("ok — v0.1.0");
    expect(screen.getByTestId("encoders")).toHaveTextContent("hevc_nvenc, av1_nvenc");
  });

  it("reports an unreachable server", async () => {
    const api = createApiClient("http://test", fakeFetch({}));
    render(<App api={api} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Server unreachable");
  });
});
