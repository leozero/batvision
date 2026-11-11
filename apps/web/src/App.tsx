import type { ApiClient, Schemas } from "@batvision/api-client";
import { useEffect, useState } from "react";

type State =
  | { kind: "loading" }
  | { kind: "error" }
  | { kind: "ready"; health: Schemas["Health"]; setup: Schemas["SetupStatus"] };

/** Placeholder until milestone 2: proves the web client reaches the server through the generated API client. */
export function App({ api }: { api: ApiClient }) {
  const [state, setState] = useState<State>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.GET("/api/health"), api.GET("/api/setup")])
      .then(([health, setup]) => {
        if (cancelled) return;
        if (health.data && setup.data) {
          setState({ kind: "ready", health: health.data, setup: setup.data });
        } else {
          setState({ kind: "error" });
        }
      })
      .catch(() => !cancelled && setState({ kind: "error" }));
    return () => {
      cancelled = true;
    };
  }, [api]);

  return (
    <main>
      <h1>batvision</h1>
      {state.kind === "loading" && <p>Connecting to the server…</p>}
      {state.kind === "error" && <p role="alert">Server unreachable</p>}
      {state.kind === "ready" && (
        <dl>
          <dt>Server</dt>
          <dd data-testid="server-status">
            {state.health.status} — v{state.health.version}
          </dd>
          <dt>Setup</dt>
          <dd data-testid="setup">
            {state.setup.needed
              ? "First launch: create the administrator account with the setup token printed in the server logs."
              : "Ready"}
          </dd>
        </dl>
      )}
    </main>
  );
}
