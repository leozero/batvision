import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Clapperboard, Search, Wand2 } from "lucide-react";
import { motion } from "motion/react";
import { type FormEvent, type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, call, imageUrl, type Schemas } from "../lib/api";
import { invalidateLibrary, meQuery } from "../lib/queries";
import { Img } from "./media";
import { Button, Dialog, ErrorText, Input, Spinner } from "./ui";

/** Backdrop + poster + title block shared by movie and series pages. */
export function DetailHeader({
  backdrop,
  poster,
  title,
  meta,
  overview,
  actions,
}: {
  backdrop?: string | null;
  poster?: string | null;
  title: string;
  meta: ReactNode;
  overview?: string | null;
  actions: ReactNode;
}) {
  return (
    <section className="relative isolate -mt-16 overflow-hidden pt-16">
      <motion.div
        className="absolute inset-0 -z-10"
        initial={{ opacity: 0, scale: 1.08 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 1.2, ease: [0.22, 1, 0.36, 1] }}
      >
        <Img src={imageUrl(backdrop, "original")} alt="" className="size-full object-cover" />
      </motion.div>
      <div className="absolute inset-0 -z-10 bg-gradient-to-t from-bg via-bg/75 to-bg/30" />
      <div className="absolute inset-0 -z-10 bg-gradient-to-r from-bg via-bg/50 to-transparent" />

      <div className="mx-auto flex max-w-6xl flex-col gap-8 px-4 pb-10 pt-12 sm:flex-row sm:items-end sm:px-10 sm:pt-28">
        <motion.div
          initial={{ opacity: 0, y: 30, rotate: -2 }}
          animate={{ opacity: 1, y: 0, rotate: 0 }}
          transition={{ type: "spring", stiffness: 120, damping: 16, delay: 0.1 }}
          className="hidden w-56 shrink-0 overflow-hidden rounded-xl shadow-2xl shadow-black/70 ring-1 ring-white/10 sm:block"
        >
          <Img
            src={imageUrl(poster, "w500")}
            alt={title}
            className="aspect-[2/3] w-full object-cover"
            fallback={<Clapperboard className="size-10 text-faint" />}
          />
        </motion.div>
        <motion.div
          className="min-w-0 flex-1"
          initial="hidden"
          animate="show"
          variants={{ show: { transition: { staggerChildren: 0.07, delayChildren: 0.15 } } }}
        >
          {[
            <h1 key="t" className="text-3xl font-black tracking-tight sm:text-5xl">
              {title}
            </h1>,
            <div
              key="m"
              className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted"
            >
              {meta}
            </div>,
            overview ? (
              <p key="o" className="mt-5 max-w-3xl text-base leading-relaxed text-fg/85">
                {overview}
              </p>
            ) : null,
            <div key="a" className="mt-7 flex flex-wrap items-center gap-3">
              {actions}
            </div>,
          ]
            .filter(Boolean)
            .map((child, i) => (
              <motion.div
                key={i}
                variants={{ hidden: { opacity: 0, y: 14 }, show: { opacity: 1, y: 0 } }}
              >
                {child}
              </motion.div>
            ))}
        </motion.div>
      </div>
    </section>
  );
}

export function Dot() {
  return <span className="size-1 rounded-full bg-faint" />;
}

/** Admin: fix a wrong TMDB match. */
export function IdentifyButton({
  kind,
  id,
  title,
  year,
}: {
  kind: "movies" | "series";
  id: number;
  title: string;
  year?: number | null;
}) {
  const { t } = useTranslation();
  const { data: me } = useQuery(meQuery);
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState(title);
  const [results, setResults] = useState<Schemas["MetadataCandidate"][] | null>(null);
  const [busy, setBusy] = useState<number | "search" | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (me?.role !== "admin") return null;

  const search = async (e?: FormEvent) => {
    e?.preventDefault();
    setBusy("search");
    setError(null);
    try {
      setResults(
        await call(
          api.GET("/api/admin/metadata/search", { params: { query: { kind, query, year } } }),
        ),
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  const pick = async (tmdbId: number) => {
    setBusy(tmdbId);
    setError(null);
    try {
      const path =
        kind === "movies" ? "/api/admin/movies/{id}/identify" : "/api/admin/series/{id}/identify";
      await call(api.POST(path, { params: { path: { id } }, body: { tmdb_id: tmdbId } }));
      invalidateLibrary(qc);
      setOpen(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <Button
        variant="ghost"
        onClick={() => {
          setOpen(true);
          if (!results) void search();
        }}
      >
        <Wand2 className="size-4" />
        {t("detail.identify")}
      </Button>
      <Dialog
        open={open}
        onOpenChange={setOpen}
        title={t("detail.identifyTitle")}
        description={t("detail.identifyHint")}
        wide
      >
        <form onSubmit={search} className="mb-4 flex gap-2">
          <Input value={query} onChange={(e) => setQuery(e.target.value)} />
          <Button type="submit" loading={busy === "search"}>
            <Search className="size-4" />
          </Button>
        </form>
        <ErrorText>{error}</ErrorText>
        <div className="mt-2 space-y-2">
          {results?.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted">{t("detail.noMatch")}</p>
          ) : null}
          {results?.map((r, i) => (
            <motion.button
              key={r.tmdb_id}
              type="button"
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: i * 0.03 }}
              onClick={() => pick(r.tmdb_id)}
              className="flex w-full cursor-pointer items-center gap-4 rounded-xl p-2 text-left transition hover:bg-white/6"
            >
              <Img
                src={imageUrl(r.poster_path, "w185")}
                alt=""
                className="h-20 w-14 shrink-0 rounded-md object-cover"
              />
              <div className="min-w-0 flex-1">
                <p className="font-semibold">
                  {r.title}{" "}
                  {r.year ? <span className="font-normal text-muted">({r.year})</span> : null}
                </p>
                <p className="line-clamp-2 text-sm text-muted">{r.overview}</p>
              </div>
              {busy === r.tmdb_id ? <Spinner className="size-5" /> : null}
            </motion.button>
          ))}
        </div>
      </Dialog>
    </>
  );
}
