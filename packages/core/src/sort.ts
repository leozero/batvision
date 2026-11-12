/** Browse page sorting (D75): each sort has a natural direction, reversible. */

export type SortOrder = "added" | "title" | "year" | "rating";
export type SortDirection = "asc" | "desc";

export const SORT_ORDERS: readonly SortOrder[] = ["added", "title", "year", "rating"];

/** A to Z for titles; newest or best first for the rest. */
export function naturalDirection(sort: SortOrder): SortDirection {
  return sort === "title" ? "asc" : "desc";
}

export function flipDirection(direction: SortDirection): SortDirection {
  return direction === "asc" ? "desc" : "asc";
}

/** A remembered sort, or the default when missing or unknown. */
export function parseSort(
  sort: string | null | undefined,
  direction: string | null | undefined,
): { sort: SortOrder; direction: SortDirection } {
  const s = SORT_ORDERS.includes(sort as SortOrder) ? (sort as SortOrder) : "added";
  const d =
    direction === "asc" || direction === "desc"
      ? (direction as SortDirection)
      : naturalDirection(s);
  return { sort: s, direction: d };
}

/** Translation key of the direction button's label: `browse.direction.title.asc`… */
export function directionLabelKey(sort: SortOrder, direction: SortDirection): string {
  return `browse.direction.${sort}.${direction}`;
}
