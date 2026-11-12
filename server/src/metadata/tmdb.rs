//! Minimal TMDB v3 client (authenticated with a v4 "API Read Access Token").

use std::time::Duration;

use reqwest::StatusCode;
use serde::{Deserialize, de::DeserializeOwned};
use tokio::sync::Semaphore;

#[derive(Debug, thiserror::Error)]
pub enum TmdbError {
    #[error("TMDB rejected the token (check BATVISION_TMDB_TOKEN)")]
    Unauthorized,
    #[error("TMDB has no such item")]
    NotFound,
    #[error("TMDB request failed: {0}")]
    Http(String),
}

impl From<reqwest::Error> for TmdbError {
    fn from(err: reqwest::Error) -> Self {
        Self::Http(err.to_string())
    }
}

pub type TmdbResult<T> = Result<T, TmdbError>;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SearchResult {
    pub id: i64,
    #[serde(alias = "name")]
    pub title: String,
    #[serde(alias = "first_air_date", default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub poster_path: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
}

impl SearchResult {
    pub fn year(&self) -> Option<i64> {
        self.release_date.as_deref()?.get(0..4)?.parse().ok()
    }
}

#[derive(Debug, Deserialize)]
struct SearchPage {
    results: Vec<SearchResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MovieDetails {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub runtime: Option<i64>,
    #[serde(default)]
    pub poster_path: Option<String>,
    #[serde(default)]
    pub backdrop_path: Option<String>,
    #[serde(default)]
    pub vote_average: Option<f64>,
    #[serde(default)]
    pub vote_count: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeasonSummary {
    pub season_number: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub poster_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TvDetails {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub first_air_date: Option<String>,
    #[serde(default)]
    pub poster_path: Option<String>,
    #[serde(default)]
    pub backdrop_path: Option<String>,
    #[serde(default)]
    pub seasons: Vec<SeasonSummary>,
    #[serde(default)]
    pub vote_average: Option<f64>,
    #[serde(default)]
    pub vote_count: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EpisodeDetails {
    pub episode_number: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub air_date: Option<String>,
    #[serde(default)]
    pub runtime: Option<i64>,
    #[serde(default)]
    pub still_path: Option<String>,
    #[serde(default)]
    pub vote_average: Option<f64>,
    #[serde(default)]
    pub vote_count: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeasonDetails {
    pub season_number: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub overview: Option<String>,
    #[serde(default)]
    pub poster_path: Option<String>,
    /// TMDB gives no vote count for a season: see [`SeasonDetails::votes`].
    #[serde(default)]
    pub vote_average: Option<f64>,
    #[serde(default)]
    pub episodes: Vec<EpisodeDetails>,
}

impl SeasonDetails {
    /// Votes behind the season average: those of its episodes.
    pub fn votes(&self) -> i64 {
        self.episodes.iter().filter_map(|e| e.vote_count).sum()
    }
}

pub struct TmdbClient {
    http: reqwest::Client,
    api_url: String,
    image_url: String,
    token: String,
    /// TMDB allows ~40 req/s; stay well below.
    limiter: Semaphore,
}

impl TmdbClient {
    pub fn new(api_url: &str, image_url: &str, token: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("batvision/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client");
        Self {
            http,
            api_url: api_url.trim_end_matches('/').to_owned(),
            image_url: image_url.trim_end_matches('/').to_owned(),
            token: token.to_owned(),
            limiter: Semaphore::new(8),
        }
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> TmdbResult<T> {
        let _permit = self.limiter.acquire().await.expect("semaphore open");
        for attempt in 0..3 {
            let res = self
                .http
                .get(format!("{}{path}", self.api_url))
                .bearer_auth(&self.token)
                .query(query)
                .send()
                .await?;
            match res.status() {
                s if s.is_success() => return Ok(res.json().await?),
                StatusCode::UNAUTHORIZED => return Err(TmdbError::Unauthorized),
                StatusCode::NOT_FOUND => return Err(TmdbError::NotFound),
                StatusCode::TOO_MANY_REQUESTS if attempt < 2 => {
                    let wait = res
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1u64)
                        .min(10);
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
                s => return Err(TmdbError::Http(format!("{path}: HTTP {s}"))),
            }
        }
        Err(TmdbError::Http(format!("{path}: rate limited")))
    }

    pub async fn search_movie(
        &self,
        query: &str,
        year: Option<i64>,
        language: &str,
    ) -> TmdbResult<Vec<SearchResult>> {
        let mut params = vec![
            ("query", query.to_owned()),
            ("language", language.to_owned()),
            ("include_adult", "false".to_owned()),
        ];
        if let Some(year) = year {
            params.push(("year", year.to_string()));
        }
        Ok(self
            .get::<SearchPage>("/search/movie", &params)
            .await?
            .results)
    }

    pub async fn search_tv(
        &self,
        query: &str,
        year: Option<i64>,
        language: &str,
    ) -> TmdbResult<Vec<SearchResult>> {
        let mut params = vec![
            ("query", query.to_owned()),
            ("language", language.to_owned()),
            ("include_adult", "false".to_owned()),
        ];
        if let Some(year) = year {
            params.push(("first_air_date_year", year.to_string()));
        }
        Ok(self.get::<SearchPage>("/search/tv", &params).await?.results)
    }

    pub async fn movie(&self, id: i64, language: &str) -> TmdbResult<MovieDetails> {
        self.get(
            &format!("/movie/{id}"),
            &[("language", language.to_owned())],
        )
        .await
    }

    pub async fn tv(&self, id: i64, language: &str) -> TmdbResult<TvDetails> {
        self.get(&format!("/tv/{id}"), &[("language", language.to_owned())])
            .await
    }

    pub async fn season(
        &self,
        tv_id: i64,
        season: i64,
        language: &str,
    ) -> TmdbResult<SeasonDetails> {
        self.get(
            &format!("/tv/{tv_id}/season/{season}"),
            &[("language", language.to_owned())],
        )
        .await
    }

    /// Downloads an image (`/abc.jpg`) at a TMDB size (`w500`, `original`…).
    pub async fn image(&self, size: &str, path: &str) -> TmdbResult<Vec<u8>> {
        let _permit = self.limiter.acquire().await.expect("semaphore open");
        let res = self
            .http
            .get(format!("{}/{size}{path}", self.image_url))
            .send()
            .await?;
        match res.status() {
            s if s.is_success() => Ok(res.bytes().await?.to_vec()),
            StatusCode::NOT_FOUND => Err(TmdbError::NotFound),
            s => Err(TmdbError::Http(format!("image {path}: HTTP {s}"))),
        }
    }
}

/// Picks the best search result: an exact year match first, then ±1 year
/// (release dates differ between countries), then TMDB's own ranking.
pub fn best_match(results: &[SearchResult], year: Option<i64>) -> Option<&SearchResult> {
    let Some(year) = year else {
        return results.first();
    };
    results
        .iter()
        .find(|r| r.year() == Some(year))
        .or_else(|| {
            results
                .iter()
                .find(|r| r.year().is_some_and(|y| (y - year).abs() <= 1))
        })
        .or_else(|| results.first())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };

    fn result(id: i64, date: &str) -> SearchResult {
        SearchResult {
            id,
            title: format!("r{id}"),
            release_date: Some(date.into()),
            poster_path: None,
            overview: None,
        }
    }

    #[test]
    fn best_match_prefers_exact_then_close_year() {
        let results = vec![
            result(1, "2000-01-01"),
            result(2, "2011-05-01"),
            result(3, "2010-07-01"),
        ];
        assert_eq!(best_match(&results, Some(2010)).unwrap().id, 3);
        assert_eq!(best_match(&results, Some(2012)).unwrap().id, 2);
        assert_eq!(best_match(&results, Some(1980)).unwrap().id, 1);
        assert_eq!(best_match(&results, None).unwrap().id, 1);
        assert!(best_match(&[], Some(2000)).is_none());
    }

    #[tokio::test]
    async fn sends_bearer_token_and_parses_search() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search/movie"))
            .and(header("authorization", "Bearer tok"))
            .and(query_param("query", "Inception"))
            .and(query_param("year", "2010"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": [{ "id": 27205, "title": "Inception", "release_date": "2010-07-15" }]
            })))
            .mount(&server)
            .await;
        let client = TmdbClient::new(&server.uri(), &server.uri(), "tok");
        let results = client
            .search_movie("Inception", Some(2010), "fr-FR")
            .await
            .unwrap();
        assert_eq!(results[0].id, 27205);
        assert_eq!(results[0].year(), Some(2010));
    }

    #[tokio::test]
    async fn maps_auth_and_not_found_errors() {
        let server = MockServer::start().await;
        Mock::given(path("/movie/1"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(path("/movie/2"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let client = TmdbClient::new(&server.uri(), &server.uri(), "bad");
        assert!(matches!(
            client.movie(1, "fr-FR").await,
            Err(TmdbError::Unauthorized)
        ));
        assert!(matches!(
            client.movie(2, "fr-FR").await,
            Err(TmdbError::NotFound)
        ));
    }

    #[tokio::test]
    async fn retries_after_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(path("/tv/5"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path("/tv/5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 5, "name": "Dark", "seasons": [{ "season_number": 1 }]
            })))
            .mount(&server)
            .await;
        let client = TmdbClient::new(&server.uri(), &server.uri(), "tok");
        let tv = client.tv(5, "fr-FR").await.unwrap();
        assert_eq!(tv.name, "Dark");
        assert_eq!(tv.seasons[0].season_number, 1);
    }
}
