import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Bookmark, BookmarkCheck, Download } from "lucide-react";
import { type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, call, type Schemas } from "../lib/api";
import { syncDesktop } from "../lib/desktop";
import { languageName } from "../lib/format";
import { devicesQuery, downloadsQuery, invalidateLibrary, preferencesQuery } from "../lib/queries";
import { errorText } from "../pages/account/AccountPage";
import { Button, Dialog, ErrorText, Field, Select } from "./ui";

type Profile = Schemas["Profile"];

/** Distinct audio languages of a file, as ISO 639-2 codes. */
export function audioLanguages(file: Schemas["FileInfo"] | null | undefined): string[] {
  const langs = (file?.media?.audio ?? []).map((a) => a.language).filter((l): l is string => !!l);
  return [...new Set(langs)];
}

function AudioSelect({
  languages,
  value,
  onChange,
}: {
  languages: string[];
  value: string;
  onChange: (v: string) => void;
}) {
  const { t, i18n } = useTranslation();
  return (
    <Field label={t("downloads.audioLanguage")}>
      <Select value={value} onChange={(e) => onChange(e.target.value)}>
        <option value="">{t("downloads.defaultTrack")}</option>
        {languages.map((code) => (
          <option key={code} value={code}>
            {languageName(code, i18n.language) ?? code}
          </option>
        ))}
      </Select>
    </Field>
  );
}

/** Default audio choice: the preferred language if the file has it. */
function useDefaultAudio(languages: string[]) {
  const { data: prefs } = useQuery(preferencesQuery);
  const preferred = prefs?.audio_language ?? "";
  const same = (a: string, b: string) =>
    a === b || (a === "fra" && b === "fre") || (a === "fre" && b === "fra");
  return languages.find((l) => preferred && same(l, preferred)) ?? "";
}

function ProfileSelect({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const { t } = useTranslation();
  return (
    <Field label={t("downloads.quality")}>
      <Select value={value} onChange={(e) => onChange(e.target.value)}>
        <option value="">{t("downloads.deviceDefault")}</option>
        <option value="max">{t("downloads.profile.max")}</option>
        <option value="mobile">{t("downloads.profile.mobile")}</option>
        <option value="saver">{t("downloads.profile.saver")}</option>
      </Select>
    </Field>
  );
}

/** The device making the request (the desktop app itself, or a browser). */
function useCurrentDeviceId() {
  const { data } = useQuery(devicesQuery);
  return data?.find((d) => d.current)?.id;
}

/** In the desktop app, this computer; in a browser, the most recently seen app. */
function defaultDevice(devices: Schemas["DeviceDownloads"][], current: number | undefined) {
  return (devices.find((d) => d.id === current) ?? devices[0])?.id;
}

function NoDevice() {
  const { t } = useTranslation();
  return (
    <div className="space-y-3 text-sm text-muted">
      <p>{t("downloads.noDevice")}</p>
      <Link to="/downloads" className="text-accent hover:underline">
        {t("downloads.title")}
      </Link>
    </div>
  );
}

/** Adds a movie to the watchlist (auto-downloaded on devices that allow it). */
export function WatchlistButton({ movie }: { movie: Schemas["MovieDetail"] }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const languages = useMemo(() => audioLanguages(movie.file), [movie.file]);
  const preferred = useDefaultAudio(languages);
  const [open, setOpen] = useState(false);
  const [audio, setAudio] = useState<string | null>(null);
  const refresh = () => {
    syncDesktop();
    invalidateLibrary(qc);
    void qc.invalidateQueries({ queryKey: ["watchlist"] });
    void qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
  };
  const add = useMutation({
    mutationFn: () =>
      call(
        api.PUT("/api/watchlist/{movie_id}", {
          params: { path: { movie_id: movie.id } },
          body: { audio_language: (audio ?? preferred) || null },
        }),
      ),
    onSuccess: () => {
      setOpen(false);
      refresh();
    },
  });
  const remove = useMutation({
    mutationFn: () =>
      call(api.DELETE("/api/watchlist/{movie_id}", { params: { path: { movie_id: movie.id } } })),
    onSuccess: refresh,
  });

  if (movie.in_watchlist) {
    return (
      <Button size="lg" variant="ghost" onClick={() => remove.mutate()} loading={remove.isPending}>
        <BookmarkCheck className="size-5 text-accent" />
        {t("watchlist.inWatchlist")}
      </Button>
    );
  }
  return (
    <>
      <Button size="lg" variant="ghost" onClick={() => setOpen(true)}>
        <Bookmark className="size-5" />
        {t("watchlist.add")}
      </Button>
      <Dialog
        open={open}
        onOpenChange={setOpen}
        title={t("watchlist.addLong")}
        description={t("watchlist.intro")}
      >
        <form
          className="flex flex-col gap-4"
          onSubmit={(e: FormEvent) => {
            e.preventDefault();
            add.mutate();
          }}
        >
          <AudioSelect languages={languages} value={audio ?? preferred} onChange={setAudio} />
          <ErrorText>{add.error ? errorText(t, add.error) : null}</ErrorText>
          <Button type="submit" variant="primary" loading={add.isPending}>
            {t("watchlist.addLong")}
          </Button>
        </form>
      </Dialog>
    </>
  );
}

/** Adds a series to the watchlist: its next unwatched episodes download (D73). */
export function SeriesWatchlistButton({ series }: { series: Schemas["SeriesDetail"] }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const toggle = useMutation({
    mutationFn: () => {
      const params = { params: { path: { series_id: series.id } } };
      return call(
        series.in_watchlist
          ? api.DELETE("/api/watchlist/series/{series_id}", params)
          : api.PUT("/api/watchlist/series/{series_id}", params),
      );
    },
    onSuccess: () => {
      syncDesktop();
      invalidateLibrary(qc);
      void qc.invalidateQueries({ queryKey: ["watchlist"] });
      void qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
    },
  });
  return (
    <Button
      size="lg"
      variant="ghost"
      onClick={() => toggle.mutate()}
      loading={toggle.isPending}
      aria-pressed={series.in_watchlist}
    >
      {series.in_watchlist ? (
        <BookmarkCheck className="size-5 text-accent" />
      ) : (
        <Bookmark className="size-5" />
      )}
      {series.in_watchlist ? t("watchlist.inWatchlist") : t("watchlist.add")}
    </Button>
  );
}

/** Downloads one movie on a chosen device. */
export function DownloadMovieButton({ movie }: { movie: Schemas["MovieDetail"] }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { data: overview } = useQuery(downloadsQuery);
  const devices = overview?.devices ?? [];
  const current = useCurrentDeviceId();
  const languages = useMemo(() => audioLanguages(movie.file), [movie.file]);
  const preferred = useDefaultAudio(languages);
  const [open, setOpen] = useState(false);
  const [device, setDevice] = useState<number | null>(null);
  const [audio, setAudio] = useState<string | null>(null);
  const [profile, setProfile] = useState("");
  const deviceId = device ?? defaultDevice(devices, current);
  const add = useMutation({
    mutationFn: () =>
      call(
        api.POST("/api/downloads/manual", {
          body: {
            device_id: deviceId as number,
            kind: "movie",
            id: movie.id,
            profile: (profile || null) as Profile | null,
            audio_language: (audio ?? preferred) || null,
          },
        }),
      ),
    onSuccess: () => {
      setOpen(false);
      void qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
      syncDesktop();
    },
  });
  return (
    <>
      <Button size="lg" variant="ghost" onClick={() => setOpen(true)}>
        <Download className="size-5" />
        {t("downloads.download")}
      </Button>
      <Dialog open={open} onOpenChange={setOpen} title={t("downloads.downloadMovie")}>
        {devices.length === 0 ? (
          <NoDevice />
        ) : (
          <form
            className="flex flex-col gap-4"
            onSubmit={(e: FormEvent) => {
              e.preventDefault();
              add.mutate();
            }}
          >
            <DeviceSelect devices={devices} value={deviceId} onChange={setDevice} />
            <AudioSelect languages={languages} value={audio ?? preferred} onChange={setAudio} />
            <ProfileSelect value={profile} onChange={setProfile} />
            <ErrorText>{add.error ? errorText(t, add.error) : null}</ErrorText>
            <Button type="submit" variant="primary" loading={add.isPending}>
              {t("downloads.download")}
            </Button>
          </form>
        )}
      </Dialog>
    </>
  );
}

function DeviceSelect({
  devices,
  value,
  onChange,
}: {
  devices: Schemas["DeviceDownloads"][];
  value: number | undefined;
  onChange: (id: number) => void;
}) {
  const { t } = useTranslation();
  return (
    <Field label={t("downloads.device")}>
      <Select value={value} onChange={(e) => onChange(Number(e.target.value))}>
        {devices.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name}
          </option>
        ))}
      </Select>
    </Field>
  );
}

/** Downloads a series on a device: the next X unwatched episodes, or one season. */
export function DownloadSeriesButton({
  series,
  episodeFile,
}: {
  series: Schemas["SeriesDetail"];
  episodeFile: Schemas["FileInfo"] | null | undefined;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { data: overview } = useQuery(downloadsQuery);
  const devices = overview?.devices ?? [];
  const current = useCurrentDeviceId();
  const languages = useMemo(() => audioLanguages(episodeFile), [episodeFile]);
  const preferred = useDefaultAudio(languages);
  const seasons = series.seasons;
  const [open, setOpen] = useState(false);
  const [device, setDevice] = useState<number | null>(null);
  const [mode, setMode] = useState<"rolling" | "season">("rolling");
  const [count, setCount] = useState(3);
  const [season, setSeason] = useState<number | null>(null);
  const [audio, setAudio] = useState<string | null>(null);
  const [profile, setProfile] = useState("");
  const deviceId = device ?? defaultDevice(devices, current);
  const seasonNumber = season ?? seasons.find((s) => s.season_number > 0)?.season_number ?? 1;
  const subscribe = useMutation({
    mutationFn: () =>
      call(
        api.POST("/api/downloads/subscriptions", {
          body: {
            device_id: deviceId as number,
            series_id: series.id,
            mode,
            count: mode === "rolling" ? count : null,
            season_number: mode === "season" ? seasonNumber : null,
            profile: (profile || null) as Profile | null,
            audio_language: (audio ?? preferred) || null,
          },
        }),
      ),
    onSuccess: () => {
      setOpen(false);
      void qc.invalidateQueries({ queryKey: downloadsQuery.queryKey });
      syncDesktop();
    },
  });
  const subscribed = devices.flatMap((d) =>
    d.subscriptions
      .filter((s) => s.series_id === series.id)
      .map((s) => ({ device: d.name, sub: s })),
  );
  return (
    <>
      <Button size="lg" variant="ghost" onClick={() => setOpen(true)}>
        <Download className="size-5" />
        {subscribed.length ? t("downloads.downloading") : t("downloads.download")}
      </Button>
      <Dialog open={open} onOpenChange={setOpen} title={t("downloads.downloadSeries")} wide>
        {devices.length === 0 ? (
          <NoDevice />
        ) : (
          <form
            className="grid gap-4 sm:grid-cols-2"
            onSubmit={(e: FormEvent) => {
              e.preventDefault();
              subscribe.mutate();
            }}
          >
            <DeviceSelect devices={devices} value={deviceId} onChange={setDevice} />
            <Field label={t("downloads.what")}>
              <Select
                value={mode}
                onChange={(e) => setMode(e.target.value as "rolling" | "season")}
              >
                <option value="rolling">{t("downloads.modeRolling")}</option>
                <option value="season">{t("downloads.modeSeason")}</option>
              </Select>
            </Field>
            {mode === "rolling" ? (
              <Field label={t("downloads.howMany")} hint={t("downloads.rollingHint")}>
                <Select value={count} onChange={(e) => setCount(Number(e.target.value))}>
                  {[1, 2, 3, 4, 5, 6, 8, 10].map((n) => (
                    <option key={n} value={n}>
                      {t("downloads.nextEpisodes", { count: n })}
                    </option>
                  ))}
                </Select>
              </Field>
            ) : (
              <Field label={t("downloads.whichSeason")} hint={t("downloads.seasonHint")}>
                <Select value={seasonNumber} onChange={(e) => setSeason(Number(e.target.value))}>
                  {seasons.map((s) => (
                    <option key={s.id} value={s.season_number}>
                      {s.season_number === 0
                        ? t("detail.specials")
                        : t("detail.season", { number: s.season_number })}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
            <AudioSelect languages={languages} value={audio ?? preferred} onChange={setAudio} />
            <ProfileSelect value={profile} onChange={setProfile} />
            <div className="sm:col-span-2">
              {subscribed.length ? (
                <p className="mb-3 text-xs text-faint">
                  {t("downloads.alreadyOn", {
                    devices: subscribed.map((s) => s.device).join(", "),
                  })}
                </p>
              ) : null}
              <ErrorText>{subscribe.error ? errorText(t, subscribe.error) : null}</ErrorText>
              <Button type="submit" variant="primary" loading={subscribe.isPending}>
                {t("downloads.download")}
              </Button>
            </div>
          </form>
        )}
      </Dialog>
    </>
  );
}
