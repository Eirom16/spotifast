//! YouTube Music renderer parser adapted from Tubefast.
//! Copyright (c) 2026 Yiğit Bozyaka; MIT (see ../LICENSE-TUBEFAST).
use serde::{Deserialize, Serialize};
use serde_json::Value;
const THUMB_WIDTH: u64 = 226;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub title: String,
    pub subtitle: String,
    pub extra: String,
    pub summary: String,
    pub duration: String,
    pub thumb: String,
    pub video_id: String,
    pub browse_id: String,
    pub artist_id: String,
    pub album_id: String,
    pub album_name: String,
    pub byline: String,
    pub release_type: Option<String>,
    pub year: Option<String>,
    pub page_type: String,
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

/// Opaque browse parameters supplied by a shelf's own navigation endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShelfLink {
    pub id: String,
    pub params: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Section {
    pub title: String,
    pub layout: Layout,
    pub items: Vec<Item>,
    pub more: Option<ShelfLink>,
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub continuation: Option<String>,
    pub header: Option<Item>,
    pub sections: Vec<Section>,
    pub partial: bool,
    pub suggestions: Vec<String>,
    pub spelling: Option<String>,
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
        .or_else(|| response_continuation(response));
    page
}

const SHELVES: [&str; 9] = [
    "musicCarouselShelfRenderer",
    "musicShelfRenderer",
    "musicPlaylistShelfRenderer",
    "musicCardShelfRenderer",
    "gridRenderer",
    "gridContinuation",
    "musicShelfContinuation",
    "musicPlaylistShelfContinuation",
    "appendContinuationItemsAction",
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
                            more: None,
                        });
                    }
                    page.sections.last_mut().unwrap().items.extend(item(child));
                } else if matches!(key, "didYouMeanRenderer" | "showingResultsForRenderer") {
                    page.spelling = [
                        text(&child["correctedQuery"]),
                        text(&child["correctedQueryText"]),
                    ]
                    .into_iter()
                    .find(|query| !query.trim().is_empty() && query.chars().count() <= 128);
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
    let children = node["contents"]
        .as_array()
        .or(node["items"].as_array())
        .or(node["continuationItems"].as_array());
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
    let basic = &node["header"]["musicCarouselShelfBasicHeaderRenderer"];
    let endpoint = [
        &basic["moreContentButton"]["buttonRenderer"]["navigationEndpoint"]["browseEndpoint"],
        &basic["title"]["runs"][0]["navigationEndpoint"]["browseEndpoint"],
        &node["title"]["runs"][0]["navigationEndpoint"]["browseEndpoint"],
    ]
    .into_iter()
    .find(|value| value["browseId"].is_string());
    Section {
        title,
        layout,
        items,
        more: endpoint.and_then(|endpoint| {
            Some(ShelfLink {
                id: endpoint["browseId"].as_str()?.to_owned(),
                params: endpoint["params"].as_str().map(str::to_owned),
            })
        }),
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
    details(node, &node["subtitle"], &mut it);
    it
}

fn is_time(value: &str) -> bool {
    value.contains(':') && value.chars().all(|c| c.is_ascii_digit() || c == ':')
}

fn is_year(value: &str) -> bool {
    value.len() == 4 && value.bytes().all(|c| c.is_ascii_digit())
}

fn release_type(value: &str) -> bool {
    matches!(value, "Album" | "Single" | "EP" | "Compilation")
}

fn details(node: &Value, byline: &Value, item: &mut Item) {
    let parts: Vec<_> = item.subtitle.split(" • ").collect();
    if item.duration.is_empty()
        && let Some(time) = parts.iter().copied().find(|part| is_time(part))
    {
        item.duration = time.to_owned();
    }
    if item.browse_id.starts_with("MPRE") {
        item.release_type = parts
            .iter()
            .copied()
            .find(|part| release_type(part))
            .map(str::to_owned);
        item.year = parts
            .iter()
            .copied()
            .find(|part| is_year(part))
            .map(str::to_owned);
    }
    let endpoint = |value: &Value| {
        first_str(
            value,
            &[
                "/browseEndpoint/browseEndpointContextSupportedConfigs/browseEndpointContextMusicConfig/pageType",
            ],
        )
    };
    item.page_type = [
        endpoint(&node["navigationEndpoint"]),
        endpoint(&node["onTap"]),
        endpoint(&node["title"]["runs"][0]["navigationEndpoint"]),
        endpoint(
            &node["flexColumns"][0]["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"][0]
                ["navigationEndpoint"],
        ),
    ]
    .into_iter()
    .find(|kind| !kind.is_empty())
    .unwrap_or_default();

    links(byline, "", item);
    for column in node["flexColumns"].as_array().into_iter().flatten().skip(1) {
        links(
            &column["musicResponsiveListItemFlexColumnRenderer"]["text"],
            "",
            item,
        );
    }
    if item.is_song() || item.browse_id.starts_with("MPRE") || item.browse_id.starts_with("VL") {
        let byline = text(byline);
        item.byline = byline
            .split(" • ")
            .find(|part| {
                !part.trim().is_empty()
                    && !is_year(part)
                    && !is_time(part)
                    && !release_type(part)
                    && !matches!(*part, "Song" | "Video" | "Playlist")
                    && (*part != item.album_name
                        || item.artists.iter().any(|(artist, _)| artist == part))
            })
            .unwrap_or_default()
            .to_owned();
        // Visible duration is rendered independently from the byline.
        item.subtitle = parts
            .into_iter()
            .filter(|part| !is_time(part) && *part != "Song")
            .collect::<Vec<_>>()
            .join(" • ");
    }
}

fn header(node: &Value) -> Item {
    let details = [
        text(&node["subtitle"]),
        text(&node["secondSubtitle"]),
        text(&node["monthlyListenerCount"]),
    ];
    let mut item = Item {
        title: text(&node["title"]),
        subtitle: details
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" • "),
        extra: text(&node["straplineTextOne"]),
        summary: [
            text(&node["secondSubtitle"]),
            text(&node["monthlyListenerCount"]),
        ]
        .into_iter()
        .find(|value| !value.is_empty())
        .unwrap_or_default(),
        thumb: thumb(&node["thumbnail"]),
        ..Item::default()
    };
    item.release_type = text(&node["subtitle"])
        .split(" • ")
        .find(|part| release_type(part))
        .map(str::to_owned);
    item.year = text(&node["subtitle"])
        .split(" • ")
        .find(|part| is_year(part))
        .map(str::to_owned);
    links(&node["straplineTextOne"], "", &mut item);
    links(&node["subtitle"], "", &mut item);
    let author = &node["facepile"]["avatarStackViewModel"];
    item.byline = author["text"]["content"]
        .as_str()
        .unwrap_or(&item.extra)
        .to_owned();
    if item.byline.is_empty() {
        item.byline = item
            .artists
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
    }
    if !item.byline.is_empty() {
        let author = item.byline.clone();
        links(&node["facepile"], &author, &mut item);
    }
    item
}

fn item(node: &Value) -> Option<Item> {
    let column = |index: usize| {
        text(&node["flexColumns"][index]["musicResponsiveListItemFlexColumnRenderer"]["text"])
    };
    let title = [text(&node["title"]), column(0)]
        .into_iter()
        .find(|s| !s.is_empty())?;
    let byline = [
        &node["subtitle"],
        &node["longBylineText"],
        &node["shortBylineText"],
        &node["flexColumns"][1]["musicResponsiveListItemFlexColumnRenderer"]["text"],
    ]
    .into_iter()
    .find(|node| !node.is_null())
    .unwrap_or(&Value::Null);
    let fixed =
        text(&node["fixedColumns"][0]["musicResponsiveListItemFixedColumnRenderer"]["text"]);
    if node["musicItemRendererDisplayPolicy"] == "MUSIC_ITEM_RENDERER_DISPLAY_POLICY_GREY_OUT" {
        return None;
    }
    let mut it = Item {
        title,
        subtitle: text(byline),
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
    it.is_artist =
        !it.is_song() && (it.browse_id.starts_with("UC") || it.browse_id.starts_with("MPLA"));
    details(node, byline, &mut it);
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
                                && !id.is_empty()
                                && it.artists.iter().all(|(_, known)| known != id)
                            {
                                it.artists.push((run_text.to_owned(), id.to_owned()));
                            }
                        }
                        Some("MUSIC_PAGE_TYPE_ALBUM")
                            if it.album_id.is_empty() && !run_text.is_empty() =>
                        {
                            it.album_id = id.to_owned();
                            it.album_name = run_text.to_owned();
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
    if let Some(value) = node["simpleText"].as_str() {
        return value.to_owned();
    }
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

// Read only list-level continuations. Menu, radio and secondary shelves may
// contain tokens unrelated to the list being browsed.
fn list_continuation(list: &Value) -> Option<String> {
    continuation(&list["continuations"]).or_else(|| {
        ["contents", "items", "continuationItems"]
            .iter()
            .filter_map(|key| list[key].as_array()?.last())
            .find_map(|last| continuation(&last["continuationItemRenderer"]))
    })
}

fn playlist_continuation(node: &Value) -> Option<String> {
    match node {
        Value::Object(map) => map
            .get("musicPlaylistShelfRenderer")
            .and_then(list_continuation)
            .or_else(|| map.values().find_map(playlist_continuation)),
        Value::Array(list) => list.iter().find_map(playlist_continuation),
        _ => None,
    }
}

fn response_continuation(node: &Value) -> Option<String> {
    match node {
        Value::Object(map) => {
            for key in SHELVES {
                if let Some(list) = map.get(key)
                    && let Some(token) = list_continuation(list)
                {
                    return Some(token);
                }
            }
            map.iter()
                .filter(|(key, _)| !ITEMS.contains(&key.as_str()) && key.as_str() != "menu")
                .find_map(|(_, value)| response_continuation(value))
        }
        Value::Array(list) => list.iter().find_map(response_continuation),
        _ => None,
    }
}

pub(crate) fn suggestions(response: &Value) -> Vec<String> {
    let mut suggestions = Vec::new();
    for section in response["contents"].as_array().into_iter().flatten() {
        for item in section["searchSuggestionsSectionRenderer"]["contents"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let suggestion = &item["searchSuggestionRenderer"];
            let query = suggestion
                .pointer("/navigationEndpoint/searchEndpoint/query")
                .and_then(Value::as_str)
                .map_or_else(|| text(&suggestion["suggestion"]), str::to_owned);
            if !query.trim().is_empty()
                && query.chars().count() <= 128
                && !suggestions.contains(&query)
            {
                suggestions.push(query);
                if suggestions.len() == 6 {
                    return suggestions;
                }
            }
        }
    }
    suggestions
}

// A home feed can contain a paged shelf of its own. Its token must not replace
// the token belonging to the complete section list displayed by the host.
fn page_continuation(node: &Value) -> Option<String> {
    match node {
        Value::Object(map) => {
            for key in ["sectionListRenderer", "sectionListContinuation"] {
                if let Some(list) = map.get(key)
                    && let Some(token) = list_continuation(list)
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
