import type { ApiClient, Schemas } from "@batvision/api-client";
import { useEffect, useState } from "react";

type State =
  | { kind: "loading" }
  | { kind: "error" }
  | { kind: "ready"; health: Schemas["Health"]; info: Schemas["SystemInfo"] };

/** Milestone 0 placeholder: proves the web client reaches the server through the generated API client. */
export function App({ api }: { api: ApiClient }) {
  const [state, setState] = useState<State>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.GET("/api/health"), api.GET("/api/system/info")])
      .then(([health, info]) => {
        if (cancelled) return;
        if (health.data && info.data) {
          setState({ kind: "ready", health: health.data, info: info.data });
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
          <dt>ffmpeg</dt>
          <dd data-testid="ffmpeg">
            {state.info.ffmpeg === null || state.info.ffmpeg === undefined
              ? "detecting…"
              : (state.info.ffmpeg.version ?? "not found")}
          </dd>
          <dt>Encoders</dt>
          <dd data-testid="encoders">{state.info.ffmpeg?.working_encoders.join(", ") || "—"}</dd>
        </dl>
      )}
    </main>
  );
}
