import { Star } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Schemas } from "../lib/api";
import { cx } from "./ui";

type RatingData = Schemas["Rating"];

const SOURCE_NAMES: Record<RatingData["source"], string> = { tmdb: "TMDB" };

/** The rating to show: TMDB's for now (the server already hides those with few votes). */
export function pickRating(ratings: RatingData[] | null | undefined): RatingData | undefined {
  return ratings?.find((r) => r.source === "tmdb") ?? ratings?.[0];
}

/** "★ 7,8": green from 7/10, red under 5/10; source and votes on hover. */
export function Rating({
  ratings,
  className,
}: {
  ratings: RatingData[] | null | undefined;
  className?: string;
}) {
  const { t, i18n } = useTranslation();
  const rating = pickRating(ratings);
  if (!rating) return null;
  const outOf10 = (rating.value / rating.scale) * 10;
  const tone = outOf10 >= 7 ? "text-success" : outOf10 < 5 ? "text-danger" : "text-fg/80";
  const value = rating.value.toLocaleString(i18n.language, {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  });
  const label = t("rating.detail", {
    value,
    scale: rating.scale,
    source: SOURCE_NAMES[rating.source] ?? rating.source,
    votes: rating.votes.toLocaleString(i18n.language),
    count: rating.votes,
  });
  return (
    <span
      title={label}
      data-testid="rating"
      className={cx("inline-flex items-center gap-1 tabular-nums", tone, className)}
    >
      <Star className="size-[0.85em]" fill="currentColor" aria-hidden />
      <span aria-hidden>{value}</span>
      <span className="sr-only">{label}</span>
    </span>
  );
}
