import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Clapperboard } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Hero, PosterCard, Row, RowSkeleton, WideCard } from "../components/media";
import { Button, EmptyState, Page, Skeleton } from "../components/ui";
import { homeQuery, meQuery } from "../lib/queries";

export function HomePage() {
  const { t } = useTranslation();
  const { data, isPending } = useQuery(homeQuery);
  const { data: me } = useQuery(meQuery);

  if (isPending) {
    return (
      <Page>
        <Skeleton className="-mt-16 mb-10 h-[70svh] rounded-none" />
        <div className="space-y-10">
          <RowSkeleton wide />
          <RowSkeleton />
        </div>
      </Page>
    );
  }
  if (!data) return null;

  const empty = !data.hero && data.recent_movies.length === 0 && data.recent_series.length === 0;
  if (empty) {
    return (
      <Page className="px-4 pt-10">
        <EmptyState icon={<Clapperboard className="size-8" />} title={t("home.emptyTitle")}>
          <p>{me?.role === "admin" ? t("home.emptyAdmin") : t("home.emptyUser")}</p>
          {me?.role === "admin" ? (
            <Link to="/admin" search={{ tab: "libraries" }}>
              <Button variant="primary" className="mt-5">
                {t("home.addLibrary")}
              </Button>
            </Link>
          ) : null}
        </EmptyState>
      </Page>
    );
  }

  return (
    <Page>
      {data.hero ? <Hero item={data.hero} /> : <div className="h-8" />}
      <div className="relative z-10 -mt-6 space-y-8">
        {data.continue_watching.length > 0 ? (
          <Row title={t("home.continueWatching")}>
            {data.continue_watching.map((item, i) => (
              <WideCard key={`${item.kind}-${item.id}`} item={item} index={i} />
            ))}
          </Row>
        ) : null}
        {data.next_up.length > 0 ? (
          <Row title={t("home.nextUp")}>
            {data.next_up.map((item, i) => (
              <WideCard key={item.id} item={item} index={i} />
            ))}
          </Row>
        ) : null}
        {data.recent_movies.length > 0 ? (
          <Row title={t("home.recentMovies")}>
            {data.recent_movies.map((movie, i) => (
              <div key={movie.id} className="w-36 shrink-0 sm:w-44">
                <PosterCard item={movie} kind="movie" index={i} />
              </div>
            ))}
          </Row>
        ) : null}
        {data.recent_series.length > 0 ? (
          <Row title={t("home.recentSeries")}>
            {data.recent_series.map((series, i) => (
              <div key={series.id} className="w-36 shrink-0 sm:w-44">
                <PosterCard item={series} kind="series" index={i} />
              </div>
            ))}
          </Row>
        ) : null}
      </div>
    </Page>
  );
}
