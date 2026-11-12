import type { SortDirection, SortOrder } from "@batvision/core/sort";
import {
  infiniteQueryOptions,
  queryOptions,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { BatvisionSync } from "../../modules/batvision-sync";
import { ApiError, api, call, type Schemas } from "./api";

export type { SortDirection, SortOrder } from "@batvision/core/sort";

/** The signed-in user, or `null` when the token is no longer valid. */
export const meQuery = queryOptions({
  queryKey: ["me"],
  queryFn: async (): Promise<Schemas["User"] | null> => {
    try {
      return await call(api().GET("/api/me"));
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) return null;
      throw e;
    }
  },
  staleTime: 60_000,
});

export const homeQuery = queryOptions({
  queryKey: ["home"],
  queryFn: () => call(api().GET("/api/home")),
});

const PAGE = 60;

export const moviesQuery = (sort: SortOrder, order: SortDirection) =>
  infiniteQueryOptions({
    queryKey: ["movies", sort, order],
    queryFn: ({ pageParam }) =>
      call(
        api().GET("/api/movies", {
          params: { query: { sort, order, limit: PAGE, offset: pageParam } },
        }),
      ),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.items.length, 0);
      return loaded < last.total ? loaded : undefined;
    },
  });

export const seriesListQuery = (sort: SortOrder, order: SortDirection) =>
  infiniteQueryOptions({
    queryKey: ["series", sort, order],
    queryFn: ({ pageParam }) =>
      call(
        api().GET("/api/series", {
          params: { query: { sort, order, limit: PAGE, offset: pageParam } },
        }),
      ),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.items.length, 0);
      return loaded < last.total ? loaded : undefined;
    },
  });

export const movieQuery = (id: number) =>
  queryOptions({
    queryKey: ["movie", id],
    queryFn: () => call(api().GET("/api/movies/{id}", { params: { path: { id } } })),
  });

export const seriesQuery = (id: number) =>
  queryOptions({
    queryKey: ["series-detail", id],
    queryFn: () => call(api().GET("/api/series/{id}", { params: { path: { id } } })),
  });

export const seasonQuery = (id: number, season: number) =>
  queryOptions({
    queryKey: ["season", id, season],
    queryFn: () =>
      call(api().GET("/api/series/{id}/seasons/{season}", { params: { path: { id, season } } })),
  });

export const episodeQuery = (id: number) =>
  queryOptions({
    queryKey: ["episode", id],
    queryFn: () => call(api().GET("/api/episodes/{id}", { params: { path: { id } } })),
  });

export const searchQuery = (q: string) =>
  queryOptions({
    queryKey: ["search", q],
    queryFn: () => call(api().GET("/api/search", { params: { query: { q } } })),
    enabled: q.trim().length > 0,
  });

export const watchlistQuery = queryOptions({
  queryKey: ["watchlist"],
  queryFn: () => call(api().GET("/api/watchlist")),
});

export const preferencesQuery = queryOptions({
  queryKey: ["preferences"],
  queryFn: () => call(api().GET("/api/me/preferences")),
});

export const downloadsQuery = queryOptions({
  queryKey: ["downloads"],
  queryFn: () => call(api().GET("/api/downloads/overview")),
});

/** Changes made here reach the engine at its next run: run it now. */
function useDownloadsChange() {
  const qc = useQueryClient();
  return () => {
    void qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
    BatvisionSync.syncNow();
  };
}

export function useSaveSettings(deviceId: number) {
  const changed = useDownloadsChange();
  return useMutation({
    mutationFn: (settings: Schemas["DeviceSettings"]) =>
      call(
        api().PUT("/api/downloads/devices/{id}/settings", {
          params: { path: { id: deviceId } },
          body: settings,
        }),
      ),
    onSuccess: changed,
  });
}

export function useManualDownload() {
  const changed = useDownloadsChange();
  return useMutation({
    mutationFn: (body: Schemas["ManualRequest"]) =>
      call(api().POST("/api/downloads/manual", { body })),
    onSuccess: changed,
  });
}

export function useSubscribe() {
  const changed = useDownloadsChange();
  return useMutation({
    mutationFn: (body: Schemas["SubscribeRequest"]) =>
      call(api().POST("/api/downloads/subscriptions", { body })),
    onSuccess: changed,
  });
}

export function useSetPlayed() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      kind,
      id,
      played,
    }: {
      kind: Schemas["MarkableKind"];
      id: number;
      played: boolean;
    }) =>
      call(
        api().PUT("/api/played/{kind}/{id}", { params: { path: { kind, id } }, body: { played } }),
      ),
    onSuccess: () => void qc.invalidateQueries(),
  });
}

/** Adds or removes a movie or series; the engine picks the change up now (D73). */
export function useWatchlist() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (v: {
      kind: "movie" | "series";
      id: number;
      add: boolean;
      audioLanguage?: string | null;
    }) => {
      if (v.kind === "series") {
        const params = { params: { path: { series_id: v.id } } };
        return call(
          v.add
            ? api().PUT("/api/watchlist/series/{series_id}", params)
            : api().DELETE("/api/watchlist/series/{series_id}", params),
        );
      }
      const params = { params: { path: { movie_id: v.id } } };
      return call(
        v.add
          ? api().PUT("/api/watchlist/{movie_id}", {
              ...params,
              body: { audio_language: v.audioLanguage ?? null },
            })
          : api().DELETE("/api/watchlist/{movie_id}", params),
      );
    },
    onSuccess: () => {
      void qc.invalidateQueries();
      BatvisionSync.syncNow();
    },
  });
}

export function useSavePreferences() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Schemas["PreferencesUpdate"]) =>
      call(api().PUT("/api/me/preferences", { body })),
    onSuccess: (prefs) => {
      qc.setQueryData(preferencesQuery.queryKey, prefs);
      BatvisionSync.syncNow();
    },
  });
}
