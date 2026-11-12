import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { AppLayout } from "./components/Layout";
import { meQuery, setupQuery } from "./lib/queries";
import { InvitePage, LoginPage, SetupPage } from "./pages/AuthPages";
import { AccountPage } from "./pages/account/AccountPage";
import { AdminPage } from "./pages/admin/AdminPage";
import { MoviesPage, SeriesListPage } from "./pages/BrowsePages";
import { HomePage } from "./pages/HomePage";
import { MovieDetailPage } from "./pages/MovieDetailPage";
import { NotFoundPage } from "./pages/NotFoundPage";
import { SearchPage } from "./pages/SearchPage";
import { SeriesDetailPage } from "./pages/SeriesDetailPage";
import { PlayerPage } from "./player/PlayerPage";

type Context = { queryClient: QueryClient };

const root = createRootRouteWithContext<Context>()({
  component: Outlet,
  notFoundComponent: NotFoundPage,
});

/** Signed-out pages. A signed-in user is sent home. */
const publicLayout = createRoute({
  getParentRoute: () => root,
  id: "public",
  beforeLoad: async ({ context }) => {
    if (await context.queryClient.ensureQueryData(meQuery)) throw redirect({ to: "/" });
  },
});

const login = createRoute({
  getParentRoute: () => publicLayout,
  path: "/login",
  validateSearch: (s: Record<string, unknown>): { next?: string } =>
    typeof s.next === "string" ? { next: s.next } : {},
  beforeLoad: async ({ context }) => {
    const setup = await context.queryClient.fetchQuery(setupQuery);
    if (setup.needed) throw redirect({ to: "/setup" });
  },
  component: LoginPage,
});

const setup = createRoute({
  getParentRoute: () => publicLayout,
  path: "/setup",
  component: SetupPage,
});

const invite = createRoute({
  getParentRoute: () => publicLayout,
  path: "/invite/$token",
  component: InvitePage,
});

/** Everything else needs a session. */
const app = createRoute({
  getParentRoute: () => root,
  id: "app",
  beforeLoad: async ({ context, location }) => {
    const me = await context.queryClient.ensureQueryData(meQuery);
    if (!me) throw redirect({ to: "/login", search: { next: location.href } });
    return { me };
  },
});

const shell = createRoute({ getParentRoute: () => app, id: "shell", component: AppLayout });

const home = createRoute({ getParentRoute: () => shell, path: "/", component: HomePage });
const movies = createRoute({ getParentRoute: () => shell, path: "/movies", component: MoviesPage });
const movie = createRoute({
  getParentRoute: () => shell,
  path: "/movies/$id",
  component: MovieDetailPage,
});
const seriesList = createRoute({
  getParentRoute: () => shell,
  path: "/series",
  component: SeriesListPage,
});
const series = createRoute({
  getParentRoute: () => shell,
  path: "/series/$id",
  validateSearch: (s: Record<string, unknown>): { season?: number } =>
    typeof s.season === "number" ? { season: s.season } : {},
  component: SeriesDetailPage,
});
const search = createRoute({
  getParentRoute: () => shell,
  path: "/search",
  validateSearch: (s: Record<string, unknown>): { q?: string } =>
    typeof s.q === "string" ? { q: s.q } : {},
  component: SearchPage,
});
const account = createRoute({
  getParentRoute: () => shell,
  path: "/account",
  validateSearch: (s: Record<string, unknown>): { tab?: string; code?: string } => ({
    tab: typeof s.tab === "string" ? s.tab : undefined,
    code: typeof s.code === "string" ? s.code : undefined,
  }),
  component: AccountPage,
});
const admin = createRoute({
  getParentRoute: () => shell,
  path: "/admin",
  beforeLoad: ({ context }) => {
    if ((context as unknown as { me: { role: string } }).me.role !== "admin")
      throw redirect({ to: "/" });
  },
  validateSearch: (s: Record<string, unknown>): { tab?: string } =>
    typeof s.tab === "string" ? { tab: s.tab } : {},
  component: AdminPage,
});

/** The player takes the whole screen: outside the shell. */
const watch = createRoute({
  getParentRoute: () => app,
  path: "/watch/$kind/$id",
  validateSearch: (s: Record<string, unknown>): { restart?: boolean } =>
    s.restart === true ? { restart: true } : {},
  component: PlayerPage,
});

const routeTree = root.addChildren([
  publicLayout.addChildren([login, setup, invite]),
  app.addChildren([
    shell.addChildren([home, movies, movie, seriesList, series, search, account, admin]),
    watch,
  ]),
]);

export function makeRouter(queryClient: QueryClient) {
  return createRouter({
    routeTree,
    context: { queryClient },
    defaultPreload: "intent",
    defaultPreloadStaleTime: 0,
    scrollRestoration: true,
  });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof makeRouter>;
  }
}
