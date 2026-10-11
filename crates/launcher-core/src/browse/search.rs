//! Search and categories (spec §3.2).
use serde::{Deserialize, Serialize};

use super::{
    api::{Api, ApiHit},
    compat::{loader_tags, ContentKind, NON_CATEGORY_TAGS, SHADER_TAG},
};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    meta::loaders::LoaderKind,
};

pub const PAGE_SIZE: u32 = 20;
pub const MAX_QUERY_CHARS: usize = 100;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortIndex {
    #[default]
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl SortIndex {
    fn as_str(self) -> &'static str {
        match self {
            SortIndex::Relevance => "relevance",
            SortIndex::Downloads => "downloads",
            SortIndex::Follows => "follows",
            SortIndex::Newest => "newest",
            SortIndex::Updated => "updated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub query: String,
    pub kind: ContentKind,
    pub minecraft: Option<String>,
    pub loader: Option<LoaderKind>,
    pub categories: Vec<String>,
    pub sort: SortIndex,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub author: String,
    pub summary: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub follows: u64,
    pub categories: Vec<String>,
    pub kind: ContentKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub total: u32,
    pub offset: u32,
}

/// Category slugs as Modrinth writes them. Anything else is dropped before it reaches a facet.
pub fn valid_category(s: &str) -> bool {
    (1..=40).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn valid_version(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}

/// Modrinth's facet filter: an AND of OR-groups, as JSON.
pub fn facets(q: &SearchQuery) -> Result<String> {
    let mut groups: Vec<Vec<String>> =
        vec![vec![format!("project_type:{}", q.kind.project_type())]];
    if let Some(mc) = &q.minecraft {
        if !valid_version(mc) {
            return Err(LauncherError::loader(
                ErrorCode::VersionInvalid,
                format!("\"{mc}\" isn't a valid version"),
            )
            .with_param("value", mc));
        }
        groups.push(vec![format!("versions:{mc}")]);
    }
    match (q.kind, q.loader) {
        (ContentKind::Shader, _) => groups.push(vec![format!("categories:{SHADER_TAG}")]),
        (ContentKind::Mod | ContentKind::Modpack, Some(loader)) => groups.push(
            loader_tags(loader, q.minecraft.as_deref())
                .into_iter()
                .map(|t| format!("categories:{t}"))
                .collect(),
        ),
        _ => {}
    }
    for category in q.categories.iter().filter(|c| valid_category(c)) {
        groups.push(vec![format!("categories:{category}")]);
    }
    groups.push(vec!["client_side!=unsupported".into()]);
    Ok(serde_json::to_string(&groups).expect("strings serialize"))
}

/// The query string for `GET /search`.
pub fn params(q: &SearchQuery) -> Result<Vec<(&'static str, String)>> {
    let query: String = q.query.trim().chars().take(MAX_QUERY_CHARS).collect();
    Ok(vec![
        ("query", query),
        ("facets", facets(q)?),
        ("index", q.sort.as_str().to_string()),
        ("offset", q.offset.to_string()),
        ("limit", PAGE_SIZE.to_string()),
    ])
}

/// The categories among Modrinth tags (loaders dropped). `project.rs` reuses it.
pub(crate) fn only_categories(tags: Vec<String>) -> Vec<String> {
    tags.into_iter()
        .filter(|c| !NON_CATEGORY_TAGS.contains(&c.as_str()))
        .collect()
}

fn hit(h: ApiHit) -> Option<SearchHit> {
    if h.client_side == "unsupported" {
        return None;
    }
    let kind = ContentKind::from_project_type(&h.project_type)?;
    let tags = if h.display_categories.is_empty() {
        h.categories
    } else {
        h.display_categories
    };
    Some(SearchHit {
        project_id: h.project_id,
        slug: h.slug,
        title: h.title,
        author: h.author,
        summary: h.description,
        icon_url: h.icon_url,
        downloads: h.downloads,
        follows: h.follows,
        categories: only_categories(tags),
        kind,
    })
}

pub async fn search(api: &Api, q: &SearchQuery) -> Result<SearchPage> {
    let page = api.search(&params(q)?).await?;
    Ok(SearchPage {
        hits: page.hits.into_iter().filter_map(hit).collect(),
        total: page.total_hits,
        offset: page.offset,
    })
}

/// The category slugs Modrinth offers for `kind`, sorted.
pub async fn categories(api: &Api, kind: ContentKind) -> Result<Vec<String>> {
    let mut names: Vec<String> = api
        .categories()
        .await?
        .into_iter()
        .filter(|c| c.project_type == kind.project_type() && valid_category(&c.name))
        .map(|c| c.name)
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(kind: ContentKind) -> SearchQuery {
        SearchQuery {
            query: String::new(),
            kind,
            minecraft: None,
            loader: None,
            categories: vec![],
            sort: SortIndex::Relevance,
            offset: 0,
        }
    }

    fn groups(q: &SearchQuery) -> Vec<Vec<String>> {
        serde_json::from_str(&facets(q).unwrap()).unwrap()
    }

    #[test]
    fn facets_follow_kind_game_and_loader() {
        let mut q = query(ContentKind::Mod);
        q.minecraft = Some("1.20.1".into());
        q.loader = Some(LoaderKind::Quilt);
        assert_eq!(
            groups(&q),
            vec![
                vec!["project_type:mod".to_string()],
                vec!["versions:1.20.1".into()],
                vec!["categories:quilt".into(), "categories:fabric".into()],
                vec!["client_side!=unsupported".into()],
            ]
        );
        let mut shader = query(ContentKind::Shader);
        shader.loader = Some(LoaderKind::Forge);
        assert_eq!(groups(&shader)[1], vec!["categories:iris".to_string()]);
        let mut rp = query(ContentKind::ResourcePack);
        rp.loader = Some(LoaderKind::Fabric);
        assert_eq!(groups(&rp).len(), 2, "resource packs ignore the loader");
    }

    #[test]
    fn facets_ignore_hostile_categories_and_trim_the_query() {
        let mut q = query(ContentKind::Mod);
        q.categories = vec![
            "adventure".into(),
            r#"x"],["project_type:modpack"#.into(),
            "UPPER".into(),
            String::new(),
        ];
        q.query = format!("  \"é{}  ", "ü".repeat(500));
        let g = groups(&q);
        assert_eq!(g[1], vec!["categories:adventure".to_string()]);
        assert_eq!(g.len(), 3, "only the valid category got through");
        let p = params(&q).unwrap();
        let sent = &p.iter().find(|(k, _)| *k == "query").unwrap().1;
        assert_eq!(sent.chars().count(), MAX_QUERY_CHARS);
        assert!(sent.starts_with("\"é"));
        q.minecraft = Some("1.20\"]".into());
        assert_eq!(facets(&q).unwrap_err().code, ErrorCode::VersionInvalid);
    }

    #[test]
    fn params_page_and_sort() {
        let mut q = query(ContentKind::Modpack);
        q.sort = SortIndex::Downloads;
        q.offset = 40;
        let p = params(&q).unwrap();
        let get = |k: &str| p.iter().find(|(key, _)| *key == k).unwrap().1.clone();
        assert_eq!(get("index"), "downloads");
        assert_eq!(get("offset"), "40");
        assert_eq!(get("limit"), "20");
    }
}
