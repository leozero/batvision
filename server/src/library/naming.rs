//! Recognizes movies and episodes from paths, following the Jellyfin/Plex naming convention:
//!
//! - `Movies/Title (Year)/Title (Year).mkv`, or a loose `Movies/Title (Year).mkv`
//! - `Shows/Name (Year)/Season 01/Name - S01E01 - Title.mkv`
//!
//! A `[tmdbid-123]` or `{tmdb-123}` tag in a name pins the TMDB id.
//! Everything here is pure: it works on relative paths, never on the filesystem.

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::LazyLock,
};

use regex::Regex;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "wmv", "ts", "m2ts", "mts", "webm", "mpg", "mpeg", "flv",
    "ogv", "3gp",
];

/// Folders holding bonus material, never episodes or the main movie.
const EXTRA_FOLDERS: &[&str] = &[
    "extras",
    "featurettes",
    "behind the scenes",
    "deleted scenes",
    "interviews",
    "scenes",
    "shorts",
    "trailers",
    "samples",
    "sample",
    "other",
    "bonus",
];

/// File-name suffixes marking extras (`Movie-trailer.mkv`, `Movie - sample.mkv`).
const EXTRA_SUFFIXES: &[&str] = &[
    "trailer",
    "sample",
    "featurette",
    "behindthescenes",
    "deleted",
    "interview",
    "scene",
    "short",
    "other",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedName {
    pub title: String,
    pub year: Option<i64>,
    pub tmdb_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedMovie {
    /// Folder (or loose file) the movie comes from, relative to the media root.
    pub source_path: PathBuf,
    pub name: ParsedName,
    /// The main video file, relative to the media root.
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedEpisode {
    pub season: i64,
    pub episode: i64,
    /// Last episode for multi-episode files (`S01E01E02`).
    pub episode_end: Option<i64>,
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedSeries {
    pub source_path: PathBuf,
    pub name: ParsedName,
    pub episodes: Vec<DetectedEpisode>,
}

static YEAR_PAREN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\((?P<year>(?:19|20)\d{2})\)").unwrap());
static TMDB_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[\[{]\s*tmdb(?:id)?\s*[-=]\s*(?P<id>\d+)\s*[\]}]").unwrap());
/// Any other bracketed tag (`[imdbid-tt123]`, `[1080p]`).
static BRACKET_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\[{][^\]}]*[\]}]").unwrap());

static EPISODE_SXXEXX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bS(?P<s>\d{1,3})[\s._-]*E(?P<e>\d{1,4})(?:(?:[\s._-]*-?[\s._-]*E|-)(?P<end>\d{1,4}))?")
        .unwrap()
});
static EPISODE_NXNN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?P<s>\d{1,2})x(?P<e>\d{2,3})\b").unwrap());
/// Episode number alone, used inside a season folder (`E05`, `Episode 5`, `05 - Titre`).
static EPISODE_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\bE|\bEp(?:isode)?[\s._-]*|^|\s-\s*)(?P<e>\d{1,3})\b").unwrap()
});
static SEASON_FOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:season|saison|series|staffel|temporada|s)[\s._-]*(?P<s>\d{1,3})$").unwrap()
});
const SPECIALS_FOLDERS: &[&str] = &["specials", "special", "extras specials", "season 0"];

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.') || name.starts_with("@eaDir") || name.starts_with("#recycle")
}

fn is_extra_folder(name: &str) -> bool {
    EXTRA_FOLDERS.contains(&name.to_lowercase().as_str())
}

fn is_extra_file(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let lower = stem.to_lowercase();
    EXTRA_SUFFIXES.iter().any(|suffix| {
        lower.ends_with(&format!("-{suffix}"))
            || lower.ends_with(&format!(" - {suffix}"))
            || lower.ends_with(&format!(".{suffix}"))
            || lower.ends_with(&format!("_{suffix}"))
            || lower == *suffix
    })
}

/// Whether a file under a library should be ignored (hidden, extra, not a video).
pub fn should_ignore(relative: &Path) -> bool {
    if !is_video(relative) || is_extra_file(relative) {
        return true;
    }
    relative.components().any(|c| match c {
        Component::Normal(part) => {
            let part = part.to_string_lossy();
            is_hidden(&part) || is_extra_folder(&part)
        }
        _ => false,
    })
}

/// Parses `Titre (2019) [tmdbid-123]`, or a scene-style `Titre.2019.1080p.BluRay`.
pub fn parse_name(raw: &str) -> ParsedName {
    let tmdb_id = TMDB_TAG.captures(raw).and_then(|c| c["id"].parse().ok());
    let without_tags = BRACKET_TAG.replace_all(raw, " ");

    if let Some(caps) = YEAR_PAREN.captures(&without_tags) {
        let year = caps["year"].parse().ok();
        let start = caps.get(0).expect("whole match").start();
        let title = clean_title(&without_tags[..start]);
        if !title.is_empty() {
            return ParsedName {
                title,
                year,
                tmdb_id,
            };
        }
    }

    // Scene style: dots/underscores as separators, the title ends at the last
    // year-looking word (but a title is never empty: "1917.2019" is 1917 from 2019).
    let spaced = without_tags.replace(['.', '_'], " ");
    let words: Vec<&str> = spaced.split_whitespace().collect();
    let is_year = |w: &str| {
        w.len() == 4
            && (w.starts_with("19") || w.starts_with("20"))
            && w.chars().all(|c| c.is_ascii_digit())
    };
    if let Some(pos) = words
        .iter()
        .skip(1)
        .rposition(|w| is_year(w))
        .map(|p| p + 1)
    {
        return ParsedName {
            title: clean_title(&words[..pos].join(" ")),
            year: words[pos].parse().ok(),
            tmdb_id,
        };
    }

    ParsedName {
        title: clean_title(&spaced),
        year: None,
        tmdb_id,
    }
}

fn clean_title(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| c == '-' || c.is_whitespace())
        .to_owned()
}

/// `(season, episode, episode_end)` from a file name, using the folder's season
/// (`Season 02`, `Specials`) when the name itself has none.
pub fn parse_episode(
    file_stem: &str,
    folder_season: Option<i64>,
) -> Option<(i64, i64, Option<i64>)> {
    if let Some(c) = EPISODE_SXXEXX.captures(file_stem) {
        let end = c
            .name("end")
            .and_then(|m| m.as_str().parse().ok())
            .filter(|end| *end > c["e"].parse::<i64>().unwrap_or(0));
        return Some((c["s"].parse().ok()?, c["e"].parse().ok()?, end));
    }
    if let Some(c) = EPISODE_NXNN.captures(file_stem) {
        return Some((c["s"].parse().ok()?, c["e"].parse().ok()?, None));
    }
    let season = folder_season?;
    // Strip a leading series title + year so "Show (2019) - 05" doesn't read 2019.
    let stem = YEAR_PAREN.replace_all(file_stem, " ");
    let c = EPISODE_ONLY.captures(stem.trim())?;
    Some((season, c["e"].parse().ok()?, None))
}

/// Season number for a folder name (`Season 01`, `Saison 2`, `S03`, `Specials` → 0).
pub fn parse_season_folder(name: &str) -> Option<i64> {
    let lower = name.trim().to_lowercase();
    if SPECIALS_FOLDERS.contains(&lower.as_str()) {
        return Some(0);
    }
    SEASON_FOLDER
        .captures(name.trim())
        .and_then(|c| c["s"].parse().ok())
}

fn first_component(relative: &Path) -> Option<&str> {
    match relative.components().next()? {
        Component::Normal(part) => part.to_str(),
        _ => None,
    }
}

/// Groups the video files of a movies library (paths relative to the library,
/// with sizes) into movies. `library_path` is prefixed to produce media-root paths.
///
/// A folder with video files is one movie (its largest file is the main one);
/// loose files at the library root are one movie each; folders without videos
/// are walked through (collections).
pub fn detect_movies(library_path: &Path, files: &[(PathBuf, u64)]) -> Vec<DetectedMovie> {
    let mut by_folder: BTreeMap<PathBuf, (PathBuf, u64)> = BTreeMap::new();
    let mut movies = Vec::new();

    for (relative, size) in files {
        if should_ignore(relative) {
            continue;
        }
        match relative.parent().filter(|p| !p.as_os_str().is_empty()) {
            None => {
                let stem = relative
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                movies.push(DetectedMovie {
                    source_path: library_path.join(relative),
                    name: parse_name(stem),
                    file: library_path.join(relative),
                });
            }
            Some(folder) => {
                let entry = by_folder
                    .entry(folder.to_path_buf())
                    .or_insert_with(|| (relative.clone(), *size));
                if *size > entry.1 {
                    *entry = (relative.clone(), *size);
                }
            }
        }
    }

    for (folder, (file, _)) in by_folder {
        let folder_name = folder
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        movies.push(DetectedMovie {
            source_path: library_path.join(&folder),
            name: parse_name(folder_name),
            file: library_path.join(file),
        });
    }
    movies.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    movies
}

/// Groups the video files of a series library into series: each top-level folder is
/// one series. Files whose episode number can't be read are skipped.
pub fn detect_series(library_path: &Path, files: &[(PathBuf, u64)]) -> Vec<DetectedSeries> {
    let mut series: BTreeMap<String, Vec<DetectedEpisode>> = BTreeMap::new();

    for (relative, _) in files {
        if should_ignore(relative) {
            continue;
        }
        // Loose files at the library root don't belong to any series folder.
        if relative.parent().is_none_or(|p| p.as_os_str().is_empty()) {
            continue;
        }
        let Some(series_folder) = first_component(relative) else {
            continue;
        };
        let folder_season = relative
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .and_then(parse_season_folder);
        let stem = relative
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let Some((season, episode, episode_end)) = parse_episode(stem, folder_season) else {
            continue;
        };
        series
            .entry(series_folder.to_owned())
            .or_default()
            .push(DetectedEpisode {
                season,
                episode,
                episode_end,
                file: library_path.join(relative),
            });
    }

    series
        .into_iter()
        .map(|(folder, mut episodes)| {
            episodes.sort_by(|a, b| {
                (a.season, a.episode, &a.file).cmp(&(b.season, b.episode, &b.file))
            });
            // Two files for the same episode: keep the first (stable, by path).
            episodes.dedup_by(|b, a| a.season == b.season && a.episode == b.episode);
            DetectedSeries {
                source_path: library_path.join(&folder),
                name: parse_name(&folder),
                episodes,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(title: &str, year: Option<i64>, tmdb_id: Option<i64>) -> ParsedName {
        ParsedName {
            title: title.into(),
            year,
            tmdb_id,
        }
    }

    #[test]
    fn parses_standard_names() {
        assert_eq!(
            parse_name("Inception (2010)"),
            name("Inception", Some(2010), None)
        );
        assert_eq!(
            parse_name("Le Fabuleux Destin d'Amélie Poulain (2001)"),
            name("Le Fabuleux Destin d'Amélie Poulain", Some(2001), None)
        );
        assert_eq!(parse_name("Dune"), name("Dune", None, None));
    }

    #[test]
    fn parses_tmdb_tags_and_ignores_other_tags() {
        assert_eq!(
            parse_name("Dune (2021) [tmdbid-438631]"),
            name("Dune", Some(2021), Some(438631))
        );
        assert_eq!(
            parse_name("Dune (2021) {tmdb-438631} [imdbid-tt1160419]"),
            name("Dune", Some(2021), Some(438631))
        );
        assert_eq!(parse_name("Dune [1080p]"), name("Dune", None, None));
    }

    #[test]
    fn parses_scene_style_names() {
        assert_eq!(
            parse_name("The.Matrix.1999.1080p.BluRay.x264"),
            name("The Matrix", Some(1999), None)
        );
        assert_eq!(
            parse_name("Blade_Runner_2049_2017"),
            name("Blade Runner 2049", Some(2017), None)
        );
    }

    #[test]
    fn a_year_in_the_title_is_not_the_release_year() {
        assert_eq!(parse_name("1917 (2019)"), name("1917", Some(2019), None));
        assert_eq!(
            parse_name("2001 A Space Odyssey (1968)"),
            name("2001 A Space Odyssey", Some(1968), None)
        );
    }

    #[test]
    fn parses_episode_patterns() {
        assert_eq!(
            parse_episode("Show - S01E02 - Pilot", None),
            Some((1, 2, None))
        );
        assert_eq!(parse_episode("show.s1e2.720p", None), Some((1, 2, None)));
        assert_eq!(parse_episode("Show S01E01E02", None), Some((1, 1, Some(2))));
        assert_eq!(
            parse_episode("Show S01E01-E02", None),
            Some((1, 1, Some(2)))
        );
        assert_eq!(parse_episode("Show S01E01-02", None), Some((1, 1, Some(2))));
        assert_eq!(parse_episode("Show 3x07", None), Some((3, 7, None)));
        assert_eq!(parse_episode("Show S10E100", None), Some((10, 100, None)));
    }

    #[test]
    fn uses_season_folder_when_name_has_no_season() {
        assert_eq!(parse_episode("E05 - Titre", Some(2)), Some((2, 5, None)));
        assert_eq!(parse_episode("Episode 5", Some(2)), Some((2, 5, None)));
        assert_eq!(parse_episode("05 - Titre", Some(2)), Some((2, 5, None)));
        assert_eq!(
            parse_episode("Show (2019) - 05", Some(1)),
            Some((1, 5, None))
        );
        assert_eq!(parse_episode("Titre sans numéro", None), None);
    }

    #[test]
    fn parses_season_folders() {
        assert_eq!(parse_season_folder("Season 01"), Some(1));
        assert_eq!(parse_season_folder("Saison 2"), Some(2));
        assert_eq!(parse_season_folder("S03"), Some(3));
        assert_eq!(parse_season_folder("Specials"), Some(0));
        assert_eq!(parse_season_folder("Behind the scenes"), None);
    }

    #[test]
    fn ignores_extras_hidden_and_non_video() {
        assert!(should_ignore(Path::new("Movie (2000)/Movie-trailer.mkv")));
        assert!(should_ignore(Path::new(
            "Movie (2000)/Extras/Making of.mkv"
        )));
        assert!(should_ignore(Path::new("Movie (2000)/Featurettes/x.mp4")));
        assert!(should_ignore(Path::new(".hidden/Movie.mkv")));
        assert!(should_ignore(Path::new("Movie (2000)/._Movie.mkv")));
        assert!(should_ignore(Path::new("Movie (2000)/Movie.nfo")));
        assert!(should_ignore(Path::new("Movie (2000)/Movie.srt")));
        assert!(!should_ignore(Path::new("Movie (2000)/Movie (2000).MKV")));
        assert!(
            !should_ignore(Path::new("Other Movie (2000)/Other Movie.mkv")),
            "only exact folder names"
        );
    }

    fn files(list: &[(&str, u64)]) -> Vec<(PathBuf, u64)> {
        list.iter().map(|(p, s)| (PathBuf::from(p), *s)).collect()
    }

    #[test]
    fn detects_movies_in_folders_loose_and_collections() {
        let found = detect_movies(
            Path::new("Films"),
            &files(&[
                ("Inception (2010)/Inception (2010).mkv", 10),
                ("Inception (2010)/Inception-sample.mkv", 1),
                ("Inception (2010)/Inception (2010).fr.srt", 1),
                ("Heat (1995).mp4", 5),
                ("Alien Collection/Alien (1979)/Alien.mkv", 7),
                ("Alien Collection/Aliens (1986)/Aliens.mkv", 8),
                ("Big (1988)/cd-copy.mkv", 3),
                ("Big (1988)/Big.mkv", 9),
            ]),
        );
        let summary: Vec<_> = found
            .iter()
            .map(|m| (m.name.title.as_str(), m.name.year, m.file.to_str().unwrap()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "Alien",
                    Some(1979),
                    "Films/Alien Collection/Alien (1979)/Alien.mkv"
                ),
                (
                    "Aliens",
                    Some(1986),
                    "Films/Alien Collection/Aliens (1986)/Aliens.mkv"
                ),
                ("Big", Some(1988), "Films/Big (1988)/Big.mkv"),
                ("Heat", Some(1995), "Films/Heat (1995).mp4"),
                (
                    "Inception",
                    Some(2010),
                    "Films/Inception (2010)/Inception (2010).mkv"
                ),
            ]
        );
        assert_eq!(found[3].source_path, PathBuf::from("Films/Heat (1995).mp4"));
        assert_eq!(
            found[4].source_path,
            PathBuf::from("Films/Inception (2010)")
        );
    }

    #[test]
    fn detects_series_and_episodes() {
        let found = detect_series(
            Path::new("Séries"),
            &files(&[
                ("The Office (2005)/Season 01/The Office - S01E02.mkv", 1),
                ("The Office (2005)/Season 01/The Office - S01E01.mkv", 1),
                ("The Office (2005)/Season 02/E01.mkv", 1),
                ("The Office (2005)/Specials/The Office - S00E01.mkv", 1),
                ("The Office (2005)/Extras/Bloopers.mkv", 1),
                ("The Office (2005)/Season 01/notes.txt", 1),
                ("Dark (2017) [tmdbid-70523]/Saison 1/Dark.S01E01E02.mkv", 1),
                ("loose-file-S01E01.mkv", 1),
                ("Broken (2020)/Season 01/no number here.mkv", 1),
            ]),
        );
        assert_eq!(
            found.len(),
            2,
            "series without readable episodes are dropped"
        );

        let dark = &found[0];
        assert_eq!(dark.name, name("Dark", Some(2017), Some(70523)));
        assert_eq!(dark.episodes.len(), 1);
        assert_eq!(dark.episodes[0].episode_end, Some(2));

        let office = &found[1];
        assert_eq!(
            office.source_path,
            PathBuf::from("Séries/The Office (2005)")
        );
        let numbers: Vec<_> = office
            .episodes
            .iter()
            .map(|e| (e.season, e.episode))
            .collect();
        assert_eq!(numbers, vec![(0, 1), (1, 1), (1, 2), (2, 1)]);
    }

    #[test]
    fn duplicate_episode_files_keep_one() {
        let found = detect_series(
            Path::new("TV"),
            &files(&[
                ("Show/Season 01/Show S01E01 720p.mkv", 1),
                ("Show/Season 01/Show S01E01 1080p.mkv", 2),
            ]),
        );
        assert_eq!(found[0].episodes.len(), 1);
    }
}
