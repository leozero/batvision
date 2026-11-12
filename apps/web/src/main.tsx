import "./styles.css";
import "./i18n";
import { MutationCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import i18n, { uiLanguageFor } from "./i18n";
import { ApiError } from "./lib/api";
import { meQuery } from "./lib/queries";
import { makeRouter } from "./router";

const queryClient: QueryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: (count, error) => !(error instanceof ApiError && error.status < 500) && count < 2,
    },
  },
  mutationCache: new MutationCache({
    onError: (error) => {
      // An expired or revoked session: back to the sign-in page.
      if (error instanceof ApiError && error.status === 401) void router.navigate({ to: "/login" });
    },
  }),
});
const router = makeRouter(queryClient);

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

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
