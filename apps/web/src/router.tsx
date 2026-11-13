import { SORT_ORDERS, type SortOrder } from "@batvision/core/sort";
import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { useEffect } from "react";
import { AppLayout } from "./components/Layout";
import { desktop, hasServer, isUnreachable } from "./lib/desktop";
import { meQuery, setupQuery } from "./lib/queries";
import { InvitePage, LoginPage, SetupPage } from "./pages/AuthPages";
import { AccountPage } from "./pages/account/AccountPage";
import { AdminPage } from "./pages/admin/AdminPage";
import { MoviesPage, SeriesListPage } from "./pages/BrowsePages";
import { ConnectPage, OfflinePage } from "./pages/DesktopPages";
import { DownloadsPage } from "./pages/DownloadsPage";
import { HomePage } from "./pages/HomePage";
import { MovieDetailPage } from "./pages/MovieDetailPage";
import { NotFoundPage } from "./pages/NotFoundPage";
import { SearchPage } from "./pages/SearchPage";
import { SeriesDetailPage } from "./pages/SeriesDetailPage";
import { WatchlistPage } from "./pages/WatchlistPage";
import { PlayerPage } from "./player/PlayerPage";

type Context = { queryClient: QueryClient };

const root = createRootRouteWithContext<Context>()({
  // Desktop app: nothing works before it knows where the server is.
  beforeLoad: ({ location }) => {
    if (!hasServer() && location.pathname !== "/connect") throw redirect({ to: "/connect" });
  },
  component: Outlet,
  notFoundComponent: NotFoundPage,
});

/** Desktop app: an unreachable server shows the offline library instead of an error. */
async function orOffline<T>(load: Promise<T>): Promise<T> {
  try {
    return await load;
  } catch (e) {
    if (desktop && isUnreachable(e)) throw redirect({ to: "/offline" });
    throw e;
  }
}

const connect = createRoute({
  getParentRoute: () => root,
  path: "/connect",
  component: ConnectPage,
});

const offline = createRoute({
  getParentRoute: () => root,
  path: "/offline",
  component: OfflinePage,
});

/** Plays a downloaded copy without the server. */
const offlineWatch = createRoute({
  getParentRoute: () => root,
  path: "/offline/watch/$kind/$id",
  component: PlayerPage,
  // A new player per item: the next episode must not inherit the previous one's position.
  remountDeps: ({ params }) => params,
});

/** Signed-out pages. A signed-in user is sent home. */
const publicLayout = createRoute({
  getParentRoute: () => root,
  id: "public",
  beforeLoad: async ({ context }) => {
    if (await orOffline(context.queryClient.ensureQueryData(meQuery))) throw redirect({ to: "/" });
  },
});

const login = createRoute({
  getParentRoute: () => publicLayout,
  path: "/login",
  validateSearch: (s: Record<string, unknown>): { next?: string } =>
    typeof s.next === "string" ? { next: s.next } : {},
  beforeLoad: async ({ context }) => {
    const setup = await orOffline(context.queryClient.fetchQuery(setupQuery));
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
    const me = await orOffline(context.queryClient.ensureQueryData(meQuery));
    if (!me) throw redirect({ to: "/login", search: { next: location.href } });
    return { me };
  },
});

const shell = createRoute({ getParentRoute: () => app, id: "shell", component: AppLayout });

const home = createRoute({ getParentRoute: () => shell, path: "/", component: HomePage });
const movies = createRoute({
  getParentRoute: () => shell,
  path: "/movies",
  // A link can pick the sort ("See all" on the top-rated rail); it's then remembered.
  validateSearch: (s: Record<string, unknown>): { sort?: SortOrder } =>
    SORT_ORDERS.includes(s.sort as SortOrder) ? { sort: s.sort as SortOrder } : {},
  component: function Movies() {
    const { sort } = movies.useSearch();
    const navigate = movies.useNavigate();
    useEffect(() => {
      if (sort) void navigate({ search: {}, replace: true });
    }, [sort, navigate]);
    return <MoviesPage initialSort={sort} />;
  },
});
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
const watchlist = createRoute({
  getParentRoute: () => shell,
  path: "/watchlist",
  component: WatchlistPage,
});
const downloads = createRoute({
  getParentRoute: () => shell,
  path: "/downloads",
  component: DownloadsPage,
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
  // A new player per item: the next episode must not inherit the previous one's position.
  remountDeps: ({ params }) => params,
});

const routeTree = root.addChildren([
  connect,
  offline,
  offlineWatch,
  publicLayout.addChildren([login, setup, invite]),
  app.addChildren([
    shell.addChildren([
      home,
      movies,
      movie,
      seriesList,
      series,
      search,
      watchlist,
      downloads,
      account,
      admin,
    ]),
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
