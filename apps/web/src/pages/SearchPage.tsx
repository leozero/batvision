import { useQuery } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Search as SearchIcon, SearchX } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { PosterCard, PosterGrid } from "../components/media";
import { EmptyState, Page, Spinner } from "../components/ui";
import { searchQuery } from "../lib/queries";

export function SearchPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { q = "" } = useSearch({ strict: false }) as { q?: string };
  const [text, setText] = useState(q);
  const [debounced, setDebounced] = useState(q);

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebounced(text);
      void navigate({ to: "/search", search: text ? { q: text } : {}, replace: true });
    }, 250);
    return () => clearTimeout(timer);
  }, [text, navigate]);

  const { data, isFetching } = useQuery(searchQuery(debounced));
  const total = (data?.movies.length ?? 0) + (data?.series.length ?? 0);

  return (
    <Page className="mx-auto max-w-[1800px] px-4 pt-6 sm:px-10">
      <div className="relative mx-auto mb-10 max-w-2xl">
        <SearchIcon className="pointer-events-none absolute left-5 top-1/2 size-5 -translate-y-1/2 text-faint" />
        <input
          // biome-ignore lint/a11y/noAutofocus: the page exists to type a search
          autoFocus
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={t("search.placeholder")}
          className="h-14 w-full rounded-full border border-line bg-surface-2 pl-14 pr-12 text-lg outline-none transition focus:border-accent/60 focus:ring-4 focus:ring-accent/15"
        />
        {isFetching ? (
          <Spinner className="absolute right-5 top-1/2 size-5 -translate-y-1/2 text-accent" />
        ) : null}
      </div>

      {debounced.trim().length >= 2 && data && total === 0 ? (
        <EmptyState
          icon={<SearchX className="size-8" />}
          title={t("search.noResults", { query: debounced })}
        />
      ) : null}

      {data && data.movies.length > 0 ? (
        <section className="mb-12">
          <h2 className="mb-4 text-lg font-semibold">{t("nav.movies")}</h2>
          <PosterGrid>
            {data.movies.map((movie, i) => (
              <PosterCard key={movie.id} item={movie} kind="movie" index={i} />
            ))}
          </PosterGrid>
        </section>
      ) : null}
      {data && data.series.length > 0 ? (
        <section>
          <h2 className="mb-4 text-lg font-semibold">{t("nav.series")}</h2>
          <PosterGrid>
            {data.series.map((series, i) => (
              <PosterCard key={series.id} item={series} kind="series" index={i} />
            ))}
          </PosterGrid>
        </section>
      ) : null}
    </Page>
  );
}
