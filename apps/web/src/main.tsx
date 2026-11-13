import "./styles.css";
import "./i18n";
import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import i18n, { uiLanguageFor } from "./i18n";
import { ApiError } from "./lib/api";
import { desktop, isUnreachable, loadDesktopInfo, reportAv1Support } from "./lib/desktop";
import { meQuery } from "./lib/queries";
import { makeRouter } from "./router";

const queryClient: QueryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: (count, error) =>
        !(error instanceof ApiError && (error.status < 500 || isUnreachable(error))) && count < 2,
    },
  },
  queryCache: new QueryCache({
    onError: (error) => {
      // Desktop app: the server went away mid-session. The player keeps playing local copies.
      const path = window.location.pathname;
      if (desktop && isUnreachable(error) && !/^\/(offline|watch)\b/.test(path))
        void router.navigate({ to: "/offline" });
      else signedOut(error);
    },
  }),
  mutationCache: new MutationCache({
    onError: signedOut,
  }),
});
const router = makeRouter(queryClient);

/**
 * An expired or revoked session: back to the sign-in page. Only that 401: a wrong
 * password on an account form is one too (`invalid_credentials`) and stays on its page.
 */
function signedOut(error: unknown) {
  if (!(error instanceof ApiError) || error.code !== "unauthenticated") return;
  if (queryClient.getQueryData(meQuery.queryKey) === null) return;
  queryClient.setQueryData(meQuery.queryKey, null);
  void router.navigate({ to: "/login" });
}

// Follow the user's language once known.
queryClient.getQueryCache().subscribe((event) => {
  if (event.query.queryKey[0] === meQuery.queryKey[0] && event.type === "updated") {
    const me = event.query.state.data as { language?: string } | null | undefined;
    if (me?.language) void i18n.changeLanguage(uiLanguageFor(me.language));
  }
});

i18n.on("languageChanged", (lng) => {
  document.documentElement.lang = lng;
});

const root = document.getElementById("root");
if (!root) throw new Error("#root missing");

await loadDesktopInfo();
void reportAv1Support();

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
