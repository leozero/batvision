//! Recognizes movies and episodes from paths, following the Jellyfin/Plex naming convention:
//!
//! - `Movies/Title (Year)/Title (Year).mkv`, or a loose `Movies/Title (Year).mkv`
//! - `Shows/Name (Year)/Season 01/Name - S01E01 - Title.mkv`
//!
//! Scene release names (`Show.S05.MULTi.1080p.WEB.x265-GRP`, `Titre (2002) MULTi VFF
//! 1080p BluRay (Alt Title)`) are understood too: release tags, languages and the
//! release group are dropped, and a season-pack folder gives its season to its episodes.
//!
//! A `[tmdbid-123]` or `{tmdb-123}` tag in a name pins the TMDB id.
//! Everything here is pure: it works on relative paths, never on the filesystem.

use std::{
    collections::{BTreeMap, HashSet},
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
/// Episode number alone, used inside a season folder (`E05`, `Episode 5`, `Show - 05`).
static EPISODE_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\bE|\bEp(?:isode)?[\s._-]*|\s-\s*)(?P<e>\d{1,3})\b").unwrap()
});
/// A leading episode number (`05 - Titre`), but not the start of a dotted title
/// such as `11.22.63`.
static EPISODE_LEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?P<e>\d{1,3})(?:$|[^\d.]|\.\D)").unwrap());
static SEASON_FOLDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:season|saison|series|staffel|temporada|s)[\s._-]*(?P<s>\d{1,3})$").unwrap()
});
const SPECIALS_FOLDERS: &[&str] = &["specials", "special", "extras specials", "season 0"];

static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:19|20)\d{2}$").unwrap());
/// A part of a movie split in several files (`Film CD1`, `Film - part2`, `Film.disc1`).
static PART_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[\s._-]+(?:cd|dvd|part|pt|disc|disk)[\s._-]?\d{1,2}$").unwrap()
});
static PAREN_GROUP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\((?P<inner>[^()]*)\)").unwrap());
/// A dot with a letter or digit on both sides: the word is dot-separated.
static INTERNAL_DOT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]\.[\p{L}\p{N}]").unwrap());
/// Dotted sequences that are one word: acronyms (`S.H.I.E.L.D`), dotted numbers
/// (`11.22.63`, `5.1`, `2.0`) and `H.264`.
static DOTTED_WHOLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:\p{L}\.)+\p{L}\b|\b\d{1,2}(?:\.\d{1,2}){1,2}\b|\bh\.26[45]\b").unwrap()
});

/// Release tags recognized in any case. Only words that never appear in titles.
const RELEASE_TAGS: &[&str] = &[
    // Resolutions and sources.
    "4k",
    "8k",
    "uhd",
    "fhd",
    "bluray",
    "blu-ray",
    "bdrip",
    "brrip",
    "bdremux",
    "remux",
    "webrip",
    "web-rip",
    "webdl",
    "web-dl",
    "hdtv",
    "hdrip",
    "dvdrip",
    "dvdr",
    "dvd5",
    "dvd9",
    "hdlight",
    "4klight",
    "mhd",
    "hdcam",
    "camrip",
    "tvrip",
    "pdtv",
    "sdtv",
    "dvdscr",
    "hdtc",
    "amzn",
    "dsnp",
    "hmax",
    "atvp",
    "pmtp",
    // Video.
    "x264",
    "x265",
    "h264",
    "h265",
    "h.264",
    "h.265",
    "hevc",
    "avc",
    "av1",
    "vp9",
    "xvid",
    "divx",
    "hi10p",
    "hdr",
    "hdr10",
    "hdr10+",
    "hdr10plus",
    "dovi",
    "dv",
    "sdr",
    // Languages and subtitles.
    "multi",
    "multi2",
    "vff",
    "vfq",
    "vfi",
    "vf",
    "vf2",
    "vfb",
    "vof",
    "vo",
    "vost",
    "vostfr",
    "truefrench",
    "subfrench",
];
/// Release tags that are also ordinary words: only recognized in capitals.
const SHOUTING_TAGS: &[&str] = &[
    "web",
    "french",
    "english",
    "german",
    "italian",
    "spanish",
    "japanese",
    "dual",
    "subbed",
    "dubbed",
    "opus",
    "nf",
    "cr",
    "adn",
    "proper",
    "repack",
    "remastered",
    "extended",
    "unrated",
    "uncut",
    "limited",
    "internal",
    "complete",
    "integrale",
    "intégrale",
    "custom",
];
static RELEASE_TAG_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:\d{3,4}[pi]|\d{1,2}-?bits?|(?:dd|ddp|dd\+|e-?ac-?3|ac-?3|aac|dts(?:-?hd)?(?:-?ma)?|dts-?x|truehd|atmos|flac|mp3|lpcm|pcm)(?:\d(?:\.\d)?)?)$",
    )
    .unwrap()
});

/// `S05`, `S01E02`, `S01E01E02`, `S01-S03`, `S01-03`.
static SEASON_MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^s\d{1,3}(?:e\d{1,4})*(?:-s?\d{1,3})?$").unwrap());
/// A whole season in one word: `S05`, `Saison2`.
static SEASON_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:s|season|saison)(?P<s>\d{1,3})$").unwrap());
/// `Saison2`, `Season02` written as one word.
static SEASON_WORD_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:season|saison)\d{1,3}$").unwrap());
const SEASON_WORDS: &[&str] = &["season", "saison", "staffel", "temporada"];
const RANGE_WORDS: &[&str] = &["-", "à", "a", "to", "&", "et", "and"];

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

/// Parses `Titre (2019) [tmdbid-123]`, or a scene-style
/// `Titre.2019.MULTi.1080p.BluRay.x265-GRP` / `Show.S05.VOSTFR.1080p.WEB`.
///
/// The title ends at the first release tag or season marker; the year is the last
/// year-looking word before that (but a title is never empty: "1917.2019" is 1917
/// from 2019). Parenthesized text that isn't a year (an alternate title such as
/// `(28 Days Later)`) is dropped.
pub fn parse_name(raw: &str) -> ParsedName {
    let tmdb_id = TMDB_TAG.captures(raw).and_then(|c| c["id"].parse().ok());
    let tokens = tokenize(raw);
    let shouting = shouting_tags_apply(&tokens);

    let cut = (1..tokens.len())
        .find(|&i| is_release_tag(&tokens[i], shouting) || is_season_marker(&tokens, i))
        .unwrap_or(tokens.len());
    let year_pos = (1..cut).rev().find(|&i| year_of(&tokens[i]).is_some());

    let (title_end, year) = match year_pos {
        Some(pos) => (pos, year_of(&tokens[pos])),
        // A year after the release tags (`Title.MULTi.2019.1080p`).
        None => (cut, tokens[cut..].iter().find_map(|t| year_of(t))),
    };
    let mut title = clean_title(&tokens[..title_end].join(" "));
    if title.is_empty() {
        title = clean_title(&tokens.join(" "));
    }
    ParsedName {
        title,
        year,
        tmdb_id,
    }
}

/// Season of a season-pack folder name (`Show.S05.1080p.WEB`, `Show Saison 2 VOSTFR`).
/// Multi-season packs (`Show.S01-S03`) and episode markers (`S05E01`) yield `None`.
pub fn parse_season_pack(raw: &str) -> Option<i64> {
    let tokens = tokenize(raw);
    let i = (1..tokens.len()).find(|&i| is_season_marker(&tokens, i))?;
    if let Some(c) = SEASON_TOKEN.captures(&tokens[i]) {
        return c["s"].parse().ok();
    }
    // `Saison 2`, unless it's a range (`Saison 1 à 3`, `Season 1-3`).
    let number = tokens.get(i + 1)?;
    let is_range = tokens
        .get(i + 2)
        .is_some_and(|next| RANGE_WORDS.contains(&next.to_lowercase().as_str()));
    if !number.chars().all(|c| c.is_ascii_digit()) || is_range {
        return None;
    }
    number.parse().ok()
}

/// Splits a name into words: brackets and non-year parentheses are dropped,
/// whitespace and underscores separate words, and so do dots between letters or
/// digits — except in acronyms (`S.W.A.T`), dotted numbers (`11.22.63`, `5.1`)
/// and `H.264`, which stay whole.
fn tokenize(raw: &str) -> Vec<String> {
    let without_tags = BRACKET_TAG.replace_all(raw, " ");
    let without_parens = PAREN_GROUP.replace_all(&without_tags, |c: &regex::Captures| {
        let inner = c["inner"].trim();
        if YEAR.is_match(inner) {
            format!(" {inner} ")
        } else if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
            // A number in parentheses is part of the title: "(500) Days of Summer".
            c[0].to_owned()
        } else {
            " ".to_owned()
        }
    });
    let spaced = without_parens.replace('_', " ");
    let dot_separated = !raw.trim().contains(char::is_whitespace);

    let mut tokens = Vec::new();
    for word in spaced.split_whitespace() {
        // In a dot-separated name, dots left at the edges by a removed tag
        // (`Title.[1080p].MULTi`) separate nothing; elsewhere they belong (`Mr. Holland`).
        let word = if dot_separated {
            word.trim_matches('.')
        } else {
            word
        };
        if word.is_empty() {
            continue;
        }
        if !INTERNAL_DOT.is_match(word) {
            tokens.push(word.to_owned());
            continue;
        }
        // Protect the dots that belong to the word, then split on the others.
        let protected =
            DOTTED_WHOLE.replace_all(word, |c: &regex::Captures| c[0].replace('.', "\u{1}"));
        tokens.extend(
            protected
                .split('.')
                .filter(|part| !part.is_empty())
                .map(|part| part.replace('\u{1}', ".")),
        );
    }
    tokens
}

fn year_of(token: &str) -> Option<i64> {
    let core = token.rsplit_once('-').map_or(token, |(before, _)| before);
    [token, core]
        .into_iter()
        .find(|t| YEAR.is_match(t))
        .and_then(|t| t.parse().ok())
}

/// Uppercase-only tags (`FRENCH`, `WEB`, `COMPLETE`) are also ordinary words, so
/// they only count when the name has lowercase words to contrast with
/// (`THE.FRENCH.CONNECTION.1971` keeps its title).
fn shouting_tags_apply(tokens: &[String]) -> bool {
    tokens
        .iter()
        .filter(|t| !is_release_tag(t, false))
        .any(|t| t.chars().any(|c| c.is_lowercase() && c != 'i'))
}

/// Whether a word is a release tag (resolution, source, codec, audio, language…),
/// possibly followed by the release group (`10bit-RYO`).
fn is_release_tag(token: &str, shouting: bool) -> bool {
    let check = |word: &str| {
        let lower = word.to_lowercase();
        if RELEASE_TAGS.contains(&lower.as_str()) || RELEASE_TAG_PATTERN.is_match(&lower) {
            return true;
        }
        // Scene style writes tags in capitals, with a lowercase `i` (`MULTi`, `iNTERNAL`).
        shouting
            && SHOUTING_TAGS.contains(&lower.as_str())
            && !word.chars().any(|c| c.is_lowercase() && c != 'i')
    };
    check(token)
        || token
            .rsplit_once('-')
            .is_some_and(|(before, _)| check(before))
}

/// Whether the word at `i` starts a season marker: `S05`, `S01E02`, `S01-S03`,
/// `Saison 2`, `Season.02`, `Saison2`.
fn is_season_marker(tokens: &[String], i: usize) -> bool {
    let token = &tokens[i];
    if SEASON_MARKER.is_match(token) || SEASON_WORD_NUMBER.is_match(token) {
        return true;
    }
    SEASON_WORDS.contains(&token.to_lowercase().as_str())
        && tokens
            .get(i + 1)
            .is_some_and(|next| next.starts_with(|c: char| c.is_ascii_digit()))
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
    let stem = stem.trim();
    if let Some(c) = EPISODE_ONLY
        .captures(stem)
        .or_else(|| EPISODE_LEADING.captures(stem))
    {
        return Some((season, c["e"].parse().ok()?, None));
    }
    // Scene style inside a season folder or pack: `Show.Name.01.MULTi.1080p` — the
    // last bare number before the release tags.
    let tokens = tokenize(file_stem);
    let shouting = shouting_tags_apply(&tokens);
    let cut = (1..tokens.len())
        .find(|&i| is_release_tag(&tokens[i], shouting))
        .unwrap_or(tokens.len());
    let episode = (1..cut)
        .rev()
        .map(|i| &tokens[i])
        .find(|t| (1..=3).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit()))?;
    Some((season, episode.parse().ok()?, None))
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

/// A file path with its size in bytes.
type SizedFile = (PathBuf, u64);

/// Groups the video files of a movies library (paths relative to the library,
/// with sizes) into movies. `library_path` is prefixed to produce media-root paths.
///
/// A folder with video files is one movie (its largest file is the main one),
/// unless its files name two dated movies or more (a saga, a box set): then each
/// file is a movie, its parts (`CD1`, `CD2`) and copies grouped by name. Loose
/// files at the library root are one movie each; folders without videos are walked
/// through (collections); videos in a folder inside a movie's folder are bonuses.
pub fn detect_movies(library_path: &Path, files: &[(PathBuf, u64)]) -> Vec<DetectedMovie> {
    let videos: Vec<&(PathBuf, u64)> = files.iter().filter(|(p, _)| !should_ignore(p)).collect();
    // Folders holding a video directly (the library root aside: it holds loose movies).
    let movie_folders: HashSet<&Path> = videos
        .iter()
        .filter_map(|(p, _)| p.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .collect();
    let mut by_folder: BTreeMap<PathBuf, Vec<(PathBuf, u64)>> = BTreeMap::new();
    let mut movies = Vec::new();

    for (relative, size) in videos {
        match relative.parent().filter(|p| !p.as_os_str().is_empty()) {
            None => movies.push(DetectedMovie {
                source_path: library_path.join(relative),
                name: parse_name(file_stem(relative)),
                file: library_path.join(relative),
            }),
            Some(folder) => {
                // A folder inside a movie's folder holds its bonus material
                // (`Apollo 13 1995 …/Apollo 13 BONUS …/…`).
                if folder
                    .ancestors()
                    .skip(1)
                    .any(|a| movie_folders.contains(a))
                {
                    continue;
                }
                by_folder
                    .entry(folder.to_path_buf())
                    .or_default()
                    .push((relative.clone(), *size));
            }
        }
    }

    for (folder, files) in by_folder {
        // Files of one movie share a name once parts (`CD1`) and release tags are gone.
        let mut groups: BTreeMap<(String, Option<i64>), Vec<SizedFile>> = BTreeMap::new();
        for (file, size) in files {
            let name = parse_name(&PART_SUFFIX.replace(file_stem(&file), ""));
            groups
                .entry((name.title.to_lowercase(), name.year))
                .or_default()
                .push((file, size));
        }
        let largest = |files: &[(PathBuf, u64)]| {
            files
                .iter()
                .max_by_key(|(_, size)| *size)
                .map(|(f, _)| f.clone())
                .expect("a group has files")
        };
        // Two dated movies or more: a saga or a box set, one movie per file.
        if groups.keys().filter(|(_, year)| year.is_some()).count() >= 2 {
            for files in groups.values() {
                let file = largest(files);
                movies.push(DetectedMovie {
                    source_path: library_path.join(&file),
                    name: parse_name(&PART_SUFFIX.replace(file_stem(&file), "")),
                    file: library_path.join(file),
                });
            }
            continue;
        }
        // One movie (maybe with copies or odd files): named after its folder.
        let all: Vec<(PathBuf, u64)> = groups.into_values().flatten().collect();
        let folder_name = folder
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        movies.push(DetectedMovie {
            source_path: library_path.join(&folder),
            name: parse_name(folder_name),
            file: library_path.join(largest(&all)),
        });
    }
    movies.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    movies
}

fn file_stem(path: &Path) -> &str {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
}

/// Groups the video files of a series library into series: each top-level folder is
/// one series. Files whose episode number can't be read are skipped. A season-pack
/// folder (`Show.S05.1080p.WEB`) gives its season to files that don't name one.
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
            .and_then(parse_season_folder)
            .or_else(|| parse_season_pack(series_folder));
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
    fn parses_scene_release_names() {
        let cases: &[(&str, &str, Option<i64>)] = &[
            // Season packs from the real library.
            (
                "Demon.Slayer.Hashira.Training.Arc.S05.MULTi.VFF.1080p.WEBRiP.AV1.10bit-RYO",
                "Demon Slayer Hashira Training Arc",
                None,
            ),
            (
                "28 Jours Plus Tard (2002) MULTi VFF 1080p 10Bit BluRay AC3 5.1 x265 (28 Days Later)",
                "28 Jours Plus Tard",
                Some(2002),
            ),
            ("better off ted", "better off ted", None),
            (
                "Better.Off.Ted.S01.COMPLETE.720p.WEB-DL",
                "Better Off Ted",
                None,
            ),
            ("11.22.63", "11.22.63", None),
            (
                "Equilibrium.[1080p].MULTi.(2002).BluRay.x264-PopHD",
                "Equilibrium",
                Some(2002),
            ),
            ("11.22.63 (2016)", "11.22.63", Some(2016)),
            ("11.22.63.S01.MULTi.1080p.WEB.x264-GRP", "11.22.63", None),
            ("1917 (2019)", "1917", Some(2019)),
            ("1917.2019.MULTi.2160p.UHD.BluRay.x265", "1917", Some(2019)),
            // French releases.
            (
                "Le.Comte.de.Monte-Cristo.2024.FRENCH.1080p.WEB.H.264-FW",
                "Le Comte de Monte-Cristo",
                Some(2024),
            ),
            (
                "Kaamelott.Premier.Volet.2021.TRUEFRENCH.1080p.BluRay.DTS-HD.MA.5.1.x264-Ulysse",
                "Kaamelott Premier Volet",
                Some(2021),
            ),
            (
                "Les.Revenants.Saison.2.FRENCH.720p.HDTV",
                "Les Revenants",
                None,
            ),
            ("Lupin Saison 3 VOSTFR 1080p", "Lupin", None),
            (
                "Intouchables (2011) VFF 1080p HDLight x264 AC3",
                "Intouchables",
                Some(2011),
            ),
            (
                "Bienvenue chez les Ch'tis (2008)",
                "Bienvenue chez les Ch'tis",
                Some(2008),
            ),
            // Anime.
            (
                "[Erai-raws] Shingeki no Kyojin - The Final Season [1080p][Multiple Subtitle]",
                "Shingeki no Kyojin - The Final Season",
                None,
            ),
            ("Spy x Family S02 VOSTFR 1080p", "Spy x Family", None),
            (
                "Frieren - Beyond Journey's End S01 MULTi 1080p WEB x264",
                "Frieren - Beyond Journey's End",
                None,
            ),
            (
                "Jujutsu.Kaisen.S02E01.VOSTFR.1080p.CR.WEB-DL.AAC2.0.H.264",
                "Jujutsu Kaisen",
                None,
            ),
            // Words that look like tags but belong to the title.
            (
                "THE.FRENCH.CONNECTION.1971.1080p.BluRay.x264",
                "THE FRENCH CONNECTION",
                Some(1971),
            ),
            (
                "The French Dispatch (2021)",
                "The French Dispatch",
                Some(2021),
            ),
            ("Charlotte's Web (2006)", "Charlotte's Web", Some(2006)),
            (
                "Mr. Holland's Opus (1995)",
                "Mr. Holland's Opus",
                Some(1995),
            ),
            ("Mr. Robot (2015)", "Mr. Robot", Some(2015)),
            (
                "Season of the Witch (2011)",
                "Season of the Witch",
                Some(2011),
            ),
            (
                "Marvel's.Agents.of.S.H.I.E.L.D.S01.1080p",
                "Marvel's Agents of S.H.I.E.L.D",
                None,
            ),
            (
                "(500) Days of Summer (2009)",
                "(500) Days of Summer",
                Some(2009),
            ),
            ("Tron.2.0.2003", "Tron 2.0", Some(2003)),
            (
                "Spider-Man.No.Way.Home.2021.1080p.WEBRip-GRP",
                "Spider-Man No Way Home",
                Some(2021),
            ),
            (
                "Blade.Runner.2049.2017.2160p.UHD.BluRay.REMUX.HDR.HEVC.Atmos",
                "Blade Runner 2049",
                Some(2017),
            ),
        ];
        for (raw, title, year) in cases {
            assert_eq!(parse_name(raw), name(title, *year, None), "{raw}");
        }
    }

    #[test]
    fn parses_season_packs() {
        let cases: &[(&str, Option<i64>)] = &[
            (
                "Demon.Slayer.Hashira.Training.Arc.S05.MULTi.VFF.1080p.WEBRiP.AV1.10bit-RYO",
                Some(5),
            ),
            ("Better.Off.Ted.S01.COMPLETE.720p.WEB-DL", Some(1)),
            ("Les.Revenants.Saison.2.FRENCH.720p.HDTV", Some(2)),
            ("Lupin Saison 3 VOSTFR 1080p", Some(3)),
            ("Show Season02 1080p", Some(2)),
            ("Show.S01-S03.MULTi.1080p", None),
            ("Show Saison 1 à 3 MULTi", None),
            ("Show.S01E02.1080p", None),
            ("The Office (2005)", None),
            ("11.22.63", None),
            ("S01", None),
        ];
        for (raw, season) in cases {
            assert_eq!(parse_season_pack(raw), *season, "{raw}");
        }
    }

    #[test]
    fn season_pack_folder_gives_its_season_to_episodes() {
        let found = detect_series(
            Path::new("Séries"),
            &files(&[
                (
                    "Demon.Slayer.Hashira.Training.Arc.S05.MULTi.VFF.1080p.WEBRiP.AV1.10bit-RYO/Demon Slayer - 01.mkv",
                    1,
                ),
                (
                    "Demon.Slayer.Hashira.Training.Arc.S05.MULTi.VFF.1080p.WEBRiP.AV1.10bit-RYO/Demon.Slayer.Hashira.Training.Arc.02.MULTi.1080p.AV1-RYO.mkv",
                    1,
                ),
                (
                    "Demon.Slayer.Hashira.Training.Arc.S05.MULTi.VFF.1080p.WEBRiP.AV1.10bit-RYO/Demon.Slayer.S05E03.MULTi.1080p.mkv",
                    1,
                ),
                ("11.22.63/Season 01/11.22.63.E04.mkv", 1),
                ("11.22.63/Season 01/11.22.63.05.MULTi.1080p.mkv", 1),
                ("better off ted/Saison 1/better.off.ted.s01e01.mkv", 1),
            ]),
        );
        let summary: Vec<_> = found
            .iter()
            .map(|s| {
                let episodes: Vec<_> = s.episodes.iter().map(|e| (e.season, e.episode)).collect();
                (s.name.title.as_str(), episodes)
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("11.22.63", vec![(1, 4), (1, 5)]),
                (
                    "Demon Slayer Hashira Training Arc",
                    vec![(5, 1), (5, 2), (5, 3)]
                ),
                ("better off ted", vec![(1, 1)]),
            ]
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
    fn splits_sagas_and_box_sets_into_movies() {
        let found = detect_movies(
            Path::new("Films"),
            &files(&[
                ("John Wick - Saga/John Wick (2014).mkv", 10),
                ("John Wick - Saga/John Wick 2 (2017).mkv", 10),
                ("John Wick - Saga/John Wick - Parabellum (2019).mkv", 10),
                (
                    "La.Momie.La.Trilogie.MULTI.1080p-gismo65/La.Momie.1999.MULTI.1080p.BluRay-gismo65.mkv",
                    10,
                ),
                (
                    "La.Momie.La.Trilogie.MULTI.1080p-gismo65/Le.Retour.De.La.Momie.2001.MULTI.1080p.BluRay-gismo65.mkv",
                    10,
                ),
                // A movie in two parts, among others: one movie.
                ("Coffret/Dune (1984) CD1.mkv", 5),
                ("Coffret/Dune (1984) CD2.mkv", 6),
                ("Coffret/Blade Runner (1982).mkv", 9),
                // An undated stray in a saga folder is a movie of its own.
                ("Coffret/[KURISU_] Demon Slayer Film MULTI 1080P.mkv", 4),
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
                    "Blade Runner",
                    Some(1982),
                    "Films/Coffret/Blade Runner (1982).mkv"
                ),
                ("Dune", Some(1984), "Films/Coffret/Dune (1984) CD2.mkv"),
                (
                    "Demon Slayer Film",
                    None,
                    "Films/Coffret/[KURISU_] Demon Slayer Film MULTI 1080P.mkv"
                ),
                (
                    "John Wick",
                    Some(2014),
                    "Films/John Wick - Saga/John Wick (2014).mkv"
                ),
                (
                    "John Wick - Parabellum",
                    Some(2019),
                    "Films/John Wick - Saga/John Wick - Parabellum (2019).mkv"
                ),
                (
                    "John Wick 2",
                    Some(2017),
                    "Films/John Wick - Saga/John Wick 2 (2017).mkv"
                ),
                (
                    "La Momie",
                    Some(1999),
                    "Films/La.Momie.La.Trilogie.MULTI.1080p-gismo65/La.Momie.1999.MULTI.1080p.BluRay-gismo65.mkv"
                ),
                (
                    "Le Retour De La Momie",
                    Some(2001),
                    "Films/La.Momie.La.Trilogie.MULTI.1080p-gismo65/Le.Retour.De.La.Momie.2001.MULTI.1080p.BluRay-gismo65.mkv"
                ),
            ]
        );
        // Each movie of a saga is identified by its own file.
        assert_eq!(found[0].source_path, found[0].file);
    }

    #[test]
    fn copies_of_one_movie_stay_one_movie() {
        let found = detect_movies(
            Path::new("Films"),
            &files(&[
                ("Heat (1995)/Heat (1995) 1080p.mkv", 5),
                ("Heat (1995)/Heat (1995) 2160p HDR.mkv", 9),
            ]),
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source_path, PathBuf::from("Films/Heat (1995)"));
        assert_eq!(
            found[0].file,
            PathBuf::from("Films/Heat (1995)/Heat (1995) 2160p HDR.mkv")
        );
    }

    #[test]
    fn a_folder_inside_a_movie_folder_holds_bonuses() {
        let found = detect_movies(
            Path::new("Films"),
            &files(&[
                (
                    "Apollo 13 1995 MULTi 1080p/Apollo 13 1995 MULTi 1080p.mkv",
                    9,
                ),
                (
                    "Apollo 13 1995 MULTi 1080p/Apollo 13 BONUS VOST 1080p/Apollo 13 BONUS Vingt Ans Apres.mkv",
                    2,
                ),
                ("Loose (2001).mkv", 3),
                // Collections still work: their folder holds no video itself.
                ("Alien Collection/Alien (1979)/Alien.mkv", 7),
            ]),
        );
        let titles: Vec<_> = found.iter().map(|m| m.name.title.as_str()).collect();
        assert_eq!(titles, vec!["Alien", "Apollo 13", "Loose"]);
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
