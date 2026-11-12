import {
  infiniteQueryOptions,
  keepPreviousData,
  queryOptions,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { ApiError, api, call, type Schemas } from "./api";

export type User = Schemas["User"];
export type SortOrder = "title" | "added" | "year";

/** The signed-in user, or `null` when signed out. */
export const meQuery = queryOptions({
  queryKey: ["me"],
  queryFn: async (): Promise<User | null> => {
    try {
      return await call(api.GET("/api/me"));
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) return null;
      throw e;
    }
  },
  staleTime: 60_000,
});

export const setupQuery = queryOptions({
  queryKey: ["setup"],
  queryFn: () => call(api.GET("/api/setup")),
});

export const homeQuery = queryOptions({
  queryKey: ["home"],
  queryFn: () => call(api.GET("/api/home")),
});

const PAGE = 60;

export const moviesQuery = (sort: SortOrder) =>
  infiniteQueryOptions({
    queryKey: ["movies", sort],
    queryFn: ({ pageParam }) =>
      call(api.GET("/api/movies", { params: { query: { sort, limit: PAGE, offset: pageParam } } })),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.items.length, 0);
      return loaded < last.total ? loaded : undefined;
    },
  });

export const seriesListQuery = (sort: SortOrder) =>
  infiniteQueryOptions({
    queryKey: ["series", sort],
    queryFn: ({ pageParam }) =>
      call(api.GET("/api/series", { params: { query: { sort, limit: PAGE, offset: pageParam } } })),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.items.length, 0);
      return loaded < last.total ? loaded : undefined;
    },
  });

export const movieQuery = (id: number) =>
  queryOptions({
    queryKey: ["movie", id],
    queryFn: () => call(api.GET("/api/movies/{id}", { params: { path: { id } } })),
  });

export const seriesQuery = (id: number) =>
  queryOptions({
    queryKey: ["series-detail", id],
    queryFn: () => call(api.GET("/api/series/{id}", { params: { path: { id } } })),
  });

export const seasonQuery = (id: number, season: number) =>
  queryOptions({
    queryKey: ["season", id, season],
    queryFn: () =>
      call(api.GET("/api/series/{id}/seasons/{season}", { params: { path: { id, season } } })),
  });

export const episodeQuery = (id: number) =>
  queryOptions({
    queryKey: ["episode", id],
    queryFn: () => call(api.GET("/api/episodes/{id}", { params: { path: { id } } })),
  });

export const searchQuery = (q: string) =>
  queryOptions({
    queryKey: ["search", q],
    queryFn: () => call(api.GET("/api/search", { params: { query: { q } } })),
    enabled: q.trim().length >= 2,
    placeholderData: keepPreviousData,
  });

export const preferencesQuery = queryOptions({
  queryKey: ["preferences"],
  queryFn: () => call(api.GET("/api/me/preferences")),
});

export const devicesQuery = queryOptions({
  queryKey: ["devices"],
  queryFn: () => call(api.GET("/api/me/devices")),
});

// Admin
export const usersQuery = queryOptions({
  queryKey: ["admin", "users"],
  queryFn: () => call(api.GET("/api/admin/users")),
});
export const invitationsQuery = queryOptions({
  queryKey: ["admin", "invitations"],
  queryFn: () => call(api.GET("/api/admin/invitations")),
});
export const librariesQuery = queryOptions({
  queryKey: ["admin", "libraries"],
  queryFn: () => call(api.GET("/api/admin/libraries")),
});
export const scansQuery = queryOptions({
  queryKey: ["admin", "scans"],
  queryFn: () => call(api.GET("/api/admin/scans")),
  refetchInterval: (q) => (q.state.data?.some((s) => s.running) ? 1500 : false),
});
export const systemQuery = queryOptions({
  queryKey: ["admin", "system"],
  queryFn: () => call(api.GET("/api/system/info")),
});

/** Marks a movie, episode, season or series (un)watched and refreshes what shows it. */
export function useSetPlayed() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: { kind: Schemas["MarkableKind"]; id: number; played: boolean }) =>
      call(
        api.PUT("/api/played/{kind}/{id}", {
          params: { path: { kind: v.kind, id: v.id } },
          body: { played: v.played },
        }),
      ),
    onSuccess: () => invalidateLibrary(qc),
  });
}

export function invalidateLibrary(qc: ReturnType<typeof useQueryClient>) {
  for (const key of ["home", "movies", "series", "movie", "series-detail", "season", "episode"]) {
    qc.invalidateQueries({ queryKey: [key] });
  }
}

// Downloads
export const downloadsQuery = queryOptions({
  queryKey: ["downloads"],
  queryFn: () => call(api.GET("/api/downloads/overview")),
  refetchInterval: (q) =>
    q.state.data?.devices.some((d) =>
      d.items.some((i) => i.status === "queued" || i.status === "encoding"),
    )
      ? 4000
      : false,
});

export const encodesQuery = queryOptions({
  queryKey: ["admin", "encodes"],
  queryFn: () => call(api.GET("/api/admin/encodes")),
  refetchInterval: (q) =>
    q.state.data?.variants.some((v) => v.status === "queued" || v.status === "encoding")
      ? 3000
      : false,
});
