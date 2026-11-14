/**
 * The list a movie was opened from (a grid, a home row, a saga), so its page can swipe to
 * the previous and the next one. Kept in memory: opened from elsewhere, there's nothing to
 * swipe to.
 */
let movies: readonly number[] = [];

export function setMovieList(ids: readonly number[]) {
  movies = ids;
}

export function movieNeighbours(id: number): { previous?: number; next?: number } {
  const i = movies.indexOf(id);
  if (i < 0) return {};
  return { previous: movies[i - 1], next: movies[i + 1] };
}
