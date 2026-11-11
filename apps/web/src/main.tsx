import { createApiClient } from "@batvision/api-client";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";

const root = document.getElementById("root");
if (!root) throw new Error("#root missing");

createRoot(root).render(
  <StrictMode>
    <App api={createApiClient()} />
  </StrictMode>,
);
