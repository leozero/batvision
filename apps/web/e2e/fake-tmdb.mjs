// A tiny stand-in for the TMDB API, so the e2e library gets metadata and ratings
// without a network or a token. Knows only the e2e library's titles.
import { createServer } from "node:http";

const port = Number(process.env.FAKE_TMDB_PORT ?? 8422);

const movies = {
  Ronin: {
    id: 8195,
    title: "Ronin",
    release_date: "1998-09-25",
    vote_average: 6.9,
    vote_count: 2400,
  },
  Heat: { id: 949, title: "Heat", release_date: "1995-12-15", vote_average: 7.9, vote_count: 7000 },
};
const dark = {
  id: 70523,
  name: "Dark",
  first_air_date: "2017-12-01",
  overview: "Un enfant disparaît.",
  seasons: [{ season_number: 1 }],
  vote_average: 8.4,
  vote_count: 6500,
};
const darkSeason1 = {
  season_number: 1,
  vote_average: 8.2,
  episodes: [
    { episode_number: 1, vote_average: 7.9, vote_count: 30 },
    // Too few votes: no rating shown.
    { episode_number: 2, vote_average: 9.1, vote_count: 3 },
  ],
};

function routes(path, query) {
  if (path === "/health") return {};
  if (path === "/search/movie") {
    const movie = movies[query.get("query")];
    return { results: movie ? [movie] : [] };
  }
  if (path === "/search/tv") return { results: query.get("query") === "Dark" ? [dark] : [] };
  const movie = Object.values(movies).find((m) => path === `/movie/${m.id}`);
  if (movie) return { ...movie, overview: `${movie.title}.` };
  if (path === `/tv/${dark.id}`) return dark;
  if (path === `/tv/${dark.id}/season/1`) return darkSeason1;
  return null;
}

createServer((req, res) => {
  const url = new URL(req.url ?? "/", `http://127.0.0.1:${port}`);
  const body = routes(url.pathname, url.searchParams);
  res.writeHead(body ? 200 : 404, { "content-type": "application/json" });
  res.end(JSON.stringify(body ?? { status_message: "not found" }));
}).listen(port, "127.0.0.1");
