//! YouTube Music renderer parser adapted from Tubefast.
//! Copyright (c) 2026 Yiğit Bozyaka; MIT (see ../LICENSE-TUBEFAST).
use serde::{Deserialize, Serialize};
use serde_json::Value;
const THUMB_WIDTH: u64 = 700;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub title: String,
    pub subtitle: String,
    pub extra: String,
    pub duration: String,
    pub thumb: String,
    pub video_id: String,
    pub browse_id: String,
    pub artist_id: String,
    pub album_id: String,
    pub is_artist: bool,
    #[serde(default)]
    pub artists: Vec<(String, String)>,
}

impl Item {
    pub fn is_song(&self) -> bool {
        !self.video_id.is_empty()
    }

    pub fn key(&self) -> &str {
        if self.is_song() {
            &self.video_id
        } else {
            &self.browse_id
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    Cards,
    Compact,
    Tracks,
    Top,
}

#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub layout: Layout,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub continuation: Option<String>,
    pub header: Option<Item>,
    pub sections: Vec<Section>,
    pub partial: bool,
}

impl Page {
    pub fn songs(&self) -> Vec<Item> {
        self.sections
            .iter()
            .flat_map(|s| &s.items)
            .filter(|i| i.is_song())
            .cloned()
            .collect()
    }
}

pub fn parse_page(response: &Value) -> Page {
    let mut page = Page::default();
    collect(response, &mut page);
    page.sections.retain(|s| !s.items.is_empty());
    page.continuation = page_continuation(response)
        .or_else(|| playlist_continuation(response))
        .or_else(|| continuation(response));
    page
}

const SHELVES: [&str; 5] = [
    "musicCarouselShelfRenderer",
    "musicShelfRenderer",
    "musicPlaylistShelfRenderer",
    "musicCardShelfRenderer",
    "gridRenderer",
];
const ITEMS: [&str; 3] = [
    "musicResponsiveListItemRenderer",
    "musicTwoRowItemRenderer",
    "playlistPanelVideoRenderer",
];
const HEADERS: [&str; 4] = [
    "musicResponsiveHeaderRenderer",
    "musicImmersiveHeaderRenderer",
    "musicVisualHeaderRenderer",
    "musicDetailHeaderRenderer",
];

fn collect(node: &Value, page: &mut Page) {
    match node {
        Value::Array(list) => list.iter().for_each(|child| collect(child, page)),
        Value::Object(map) => {
            for (key, child) in map {
                let key = key.as_str();
                if SHELVES.contains(&key) {
                    page.sections.push(shelf(key, child));
                } else if ITEMS.contains(&key) {
                    let loose = matches!(page.sections.last(), Some(s) if s.title.is_empty() && s.layout == Layout::Tracks);
                    if !loose {
                        page.sections.push(Section {
                            title: String::new(),
                            layout: Layout::Tracks,
                            items: Vec::new(),
                        });
                    }
                    page.sections.last_mut().unwrap().items.extend(item(child));
                } else if HEADERS.contains(&key) {
                    page.header = Some(header(child));
                } else {
                    collect(child, page);
                }
            }
        }
        _ => {}
    }
}

fn shelf(kind: &str, node: &Value) -> Section {
    let children = node["contents"].as_array().or(node["items"].as_array());
    let entries = || {
        children
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .flat_map(|o| o.iter())
    };
    let mut items: Vec<Item> = entries()
        .filter(|(k, _)| ITEMS.contains(&k.as_str()))
        .filter_map(|(_, v)| item(v))
        .collect();
    let cards = entries()
        .next()
        .is_some_and(|(k, _)| k == "musicTwoRowItemRenderer");
    let (title, layout) = match kind {
        "musicCardShelfRenderer" => {
            let top = card(node);
            for item in items
                .iter_mut()
                .filter(|item| top.is_artist && item.subtitle.is_empty())
            {
                item.subtitle = top.title.clone();
            }
            items.insert(0, top);
            ("Top result".to_owned(), Layout::Top)
        }
        "musicCarouselShelfRenderer" | "gridRenderer" => {
            let basic = &node["header"]["musicCarouselShelfBasicHeaderRenderer"]["title"];
            let grid = &node["header"]["gridHeaderRenderer"]["title"];
            (
                text(basic) + text(grid).as_str(),
                if cards {
                    Layout::Cards
                } else {
                    Layout::Compact
                },
            )
        }
        _ => (text(&node["title"]), Layout::Tracks),
    };
    Section {
        title,
        layout,
        items,
    }
}

fn card(node: &Value) -> Item {
    let mut it = Item {
        title: text(&node["title"]),
        subtitle: text(&node["subtitle"]),
        thumb: thumb(&node["thumbnail"]),
        video_id: first_str(
            node,
            &[
                "/onTap/watchEndpoint/videoId",
                "/title/runs/0/navigationEndpoint/watchEndpoint/videoId",
            ],
        ),
        browse_id: first_str(
            node,
            &[
                "/onTap/browseEndpoint/browseId",
                "/title/runs/0/navigationEndpoint/browseEndpoint/browseId",
            ],
        ),
        ..Item::default()
    };
    it.is_artist = !it.is_song() && it.browse_id.starts_with("UC");
    tidy(&mut it);
    it
}

fn tidy(it: &mut Item) {
    let mut parts: Vec<&str> = it.subtitle.split(" • ").collect();
    if parts.first() == Some(&"Song") {
        parts.remove(0);
    }
    let timed = parts.last().is_some_and(|last| {
        last.contains(':') && last.chars().all(|c| c.is_ascii_digit() || c == ':')
    });
    if timed && it.duration.is_empty() {
        it.duration = parts.pop().unwrap_or_default().to_owned();
    }
    let credited;
    if it.is_song()
        && let Some(slot) = parts.iter().position(|part| *part != "Video")
    {
        let (title, featured) = clean_title(&it.title);
        let prefix = format!("{} - ", parts[slot]).to_ascii_lowercase();
        it.title = match title.to_ascii_lowercase().starts_with(&prefix) {
            true if title.len() > prefix.len() => title[prefix.len()..].to_owned(),
            _ => title,
        };
        let first = featured.split([',', '&']).next().unwrap_or_default().trim();
        if !first.is_empty() && !it.subtitle.contains(first) {
            credited = format!("{}, {featured}", parts[slot]);
            parts[slot] = &credited;
        }
    }
    it.subtitle = parts.join(" • ");
}

/// Drops the "(Official Video)" style noise from a song title and returns the
/// featured artists it named separately, so the byline can carry them.
fn clean_title(title: &str) -> (String, String) {
    let credit = |inner: &str| {
        let inner = inner.trim();
        let lower = inner.to_ascii_lowercase();
        ["featuring ", "feat.", "feat ", "ft.", "ft "]
            .into_iter()
            .find(|mark| lower.starts_with(mark))
            .map(|mark| inner[mark.len()..].trim().to_owned())
    };
    let noise = |inner: &str| {
        let lower = inner.to_ascii_lowercase();
        let tagged = ["official", "oficial", "officiel", "lyric"]
            .iter()
            .any(|word| lower.contains(word));
        let words = [
            "audio",
            "video",
            "music",
            "visualizer",
            "visualiser",
            "hd",
            "hq",
            "4k",
            "mv",
            "clip",
            "videoclip",
        ];
        let bare =
            !lower.trim().is_empty() && lower.split_whitespace().all(|word| words.contains(&word));
        let kept = [
            "remix", "live", "version", "edit", "cover", "acoustic", "mix",
        ]
        .iter()
        .any(|word| lower.contains(word));
        (tagged || bare) && !kept
    };
    let (mut kept, mut featured, mut rest) = (String::new(), String::new(), title);
    while let Some(open) = rest.find(['(', '[']) {
        let close = if rest.as_bytes()[open] == b'(' {
            ')'
        } else {
            ']'
        };
        let Some(length) = rest[open..].find(close) else {
            break;
        };
        let inner = &rest[open + 1..open + length];
        kept.push_str(&rest[..open]);
        match credit(inner) {
            Some(names) if featured.is_empty() => featured = names,
            Some(_) => {}
            None if noise(inner) => {}
            None => kept.push_str(&rest[open..=open + length]),
        }
        rest = &rest[open + length + 1..];
    }
    kept.push_str(rest);
    let lower = kept.to_ascii_lowercase();
    let inline = [" featuring ", " feat. ", " ft. "]
        .into_iter()
        .filter_map(|mark| lower.find(mark).map(|at| (at, mark.len())))
        .min();
    if let Some((at, length)) = inline.filter(|_| featured.is_empty()) {
        let tail = &kept[at + length..];
        let end = tail.find(['(', '[']).unwrap_or(tail.len());
        featured = tail[..end].trim().to_owned();
        kept = format!("{} {}", &kept[..at], &tail[end..]);
    }
    let clean = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean = clean.trim_end_matches([' ', '-', '|', '–']).to_owned();
    if clean.is_empty() {
        (title.to_owned(), String::new())
    } else {
        (clean, featured)
    }
}

fn header(node: &Value) -> Item {
    let details = [
        text(&node["subtitle"]),
        text(&node["secondSubtitle"]),
        text(&node["monthlyListenerCount"]),
    ];
    Item {
        title: text(&node["title"]),
        subtitle: details
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" • "),
        extra: text(&node["straplineTextOne"]),
        thumb: thumb(&node["thumbnail"]),
        ..Item::default()
    }
}

fn item(node: &Value) -> Option<Item> {
    let column = |index: usize| {
        text(&node["flexColumns"][index]["musicResponsiveListItemFlexColumnRenderer"]["text"])
    };
    let title = [text(&node["title"]), column(0)]
        .into_iter()
        .find(|s| !s.is_empty())?;
    let bylines = [
        text(&node["subtitle"]),
        text(&node["shortBylineText"]),
        text(&node["longBylineText"]),
        column(1),
    ];
    let subtitle = bylines.into_iter().find(|s| !s.is_empty());
    let fixed =
        text(&node["fixedColumns"][0]["musicResponsiveListItemFixedColumnRenderer"]["text"]);
    let mut it = Item {
        title,
        subtitle: subtitle.unwrap_or_default(),
        extra: [column(2), column(3)]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" • "),
        duration: fixed + text(&node["lengthText"]).as_str(),
        thumb: thumb(node),
        video_id: first_str(
            node,
            &[
                "/playlistItemData/videoId",
                "/videoId",
                "/navigationEndpoint/watchEndpoint/videoId",
                "/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/watchEndpoint/videoId",
                "/overlay/musicItemThumbnailOverlayRenderer/content/musicPlayButtonRenderer/playNavigationEndpoint/watchEndpoint/videoId",
            ],
        ),
        browse_id: first_str(
            node,
            &[
                "/navigationEndpoint/browseEndpoint/browseId",
                "/title/runs/0/navigationEndpoint/browseEndpoint/browseId",
                "/flexColumns/0/musicResponsiveListItemFlexColumnRenderer/text/runs/0/navigationEndpoint/browseEndpoint/browseId",
            ],
        ),
        ..Item::default()
    };
    links(node, "", &mut it);
    it.is_artist = !it.is_song() && it.browse_id.starts_with("UC");
    tidy(&mut it);
    (it.is_song() || !it.browse_id.is_empty()).then_some(it)
}

fn links(node: &Value, run_text: &str, it: &mut Item) {
    match node {
        Value::Array(list) => list.iter().for_each(|child| links(child, run_text, it)),
        Value::Object(map) => {
            let run_text = map.get("text").and_then(Value::as_str).unwrap_or(run_text);
            for (key, child) in map {
                if key == "browseEndpoint" {
                    let kind = child.pointer("/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType");
                    let id = child["browseId"].as_str().unwrap_or_default();
                    match kind.and_then(Value::as_str) {
                        Some("MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL") => {
                            if it.artist_id.is_empty() {
                                it.artist_id = id.to_owned();
                            }
                            if !run_text.is_empty()
                                && it.artists.iter().all(|(_, known)| known != id)
                            {
                                it.artists.push((run_text.to_owned(), id.to_owned()));
                            }
                        }
                        Some("MUSIC_PAGE_TYPE_ALBUM") if it.album_id.is_empty() => {
                            it.album_id = id.to_owned()
                        }
                        _ => {}
                    }
                } else if key != "menu" {
                    links(child, run_text, it);
                }
            }
        }
        _ => {}
    }
}

fn text(node: &Value) -> String {
    node["runs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|run| run["text"].as_str())
        .collect()
}

fn first_str(node: &Value, pointers: &[&str]) -> String {
    pointers
        .iter()
        .find_map(|p| node.pointer(p)?.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn thumb(node: &Value) -> String {
    match node {
        Value::Array(list) => list
            .iter()
            .map(thumb)
            .find(|url| !url.is_empty())
            .unwrap_or_default(),
        Value::Object(map) => {
            let sizes = map.get("thumbnails").and_then(Value::as_array);
            let enough = |size: &&Value| {
                size["width"]
                    .as_u64()
                    .is_some_and(|width| width >= THUMB_WIDTH)
            };
            if let Some(url) = sizes
                .and_then(|sizes| sizes.iter().find(enough).or(sizes.last()))
                .and_then(|size| size["url"].as_str())
            {
                return url.to_owned();
            }
            map.iter()
                .filter(|(key, _)| *key != "menu")
                .map(|(_, child)| thumb(child))
                .find(|url| !url.is_empty())
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}

fn continuation(node: &Value) -> Option<String> {
    match node {
        Value::Object(map) => {
            for key in ["nextContinuationData", "continuationCommand"] {
                if let Some(data) = map.get(key)
                    && let Some(token) = data["continuation"]
                        .as_str()
                        .or_else(|| data["token"].as_str())
                {
                    return Some(token.into());
                }
            }
            map.values().find_map(continuation)
        }
        Value::Array(list) => list.iter().find_map(continuation),
        _ => None,
    }
}

fn playlist_continuation(node: &Value) -> Option<String> {
    match node {
        Value::Object(map) => map
            .get("musicPlaylistShelfRenderer")
            .and_then(continuation)
            .or_else(|| map.values().find_map(playlist_continuation)),
        Value::Array(list) => list.iter().find_map(playlist_continuation),
        _ => None,
    }
}

// A home feed can contain a paged shelf of its own. Its token must not replace
// the token belonging to the complete section list displayed by the host.
fn page_continuation(node: &Value) -> Option<String> {
    match node {
        Value::Object(map) => {
            for key in ["sectionListRenderer", "sectionListContinuation"] {
                if let Some(list) = map.get(key)
                    && let Some(token) = continuation(&list["continuations"])
                {
                    return Some(token);
                }
            }
            map.values().find_map(page_continuation)
        }
        Value::Array(list) => list.iter().find_map(page_continuation),
        _ => None,
    }
}

#[cfg(test)]
mod continuation_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn a_whole_home_page_continuation_takes_priority_over_a_nested_playlist_shelf() {
        let shelf = json!({"musicPlaylistShelfRenderer":{"contents":[],"continuations":[{"nextContinuationData":{"continuation":"shelf-token"}}]}});
        let response = json!({"contents":{"sectionListRenderer":{"contents":[shelf.clone()],"continuations":[{"nextContinuationData":{"continuation":"home-token"}}]}}});
        assert_eq!(
            parse_page(&response).continuation.as_deref(),
            Some("home-token")
        );
        assert_eq!(
            parse_page(&shelf).continuation.as_deref(),
            Some("shelf-token")
        );
    }
}
