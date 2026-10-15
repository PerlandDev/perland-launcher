//! The player's own order for the instance list, kept in `instance-order.json` (not in settings,
//! because settings are written as one whole object).
use std::{collections::HashSet, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::write_atomic,
    instances::Instance,
};

/// Sort `instances` in place and return the full resulting id list, which is what to store.
///
/// Instances not in `order` come first, A-Z by lowercased name (id breaks ties), so a new
/// instance lands on top and an empty order gives plain A-Z. Then come the listed ones in
/// `order`. Ids in `order` that match no instance are dropped.
pub fn apply_order(instances: &mut Vec<Instance>, order: &[String]) -> Vec<String> {
    let known: HashSet<&str> = instances.iter().map(|i| i.id.as_str()).collect();
    let mut seen = HashSet::new();
    let ranked: Vec<&str> = order
        .iter()
        .map(String::as_str)
        .filter(|id| known.contains(id) && seen.insert(*id))
        .collect();
    let rank = |id: &str| ranked.iter().position(|r| *r == id);
    let mut all = std::mem::take(instances);
    all.sort_by(|a, b| match (rank(&a.id), rank(&b.id)) {
        (None, None) => (a.name.to_lowercase(), &a.id).cmp(&(b.name.to_lowercase(), &b.id)),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(x), Some(y)) => x.cmp(&y),
    });
    *instances = all;
    instances.iter().map(|i| i.id.clone()).collect()
}

/// `saved` (the order about to be stored) with the ids of `stored` that are in `keep` put back
/// beside the stored id they followed, so a damaged instance keeps its place until it's repaired.
/// Kept ids that led the stored order go before the first stored id that follows them; with
/// nothing to follow they go last. Other ids missing from `saved` are dropped.
pub fn keep_in_place(saved: &[String], stored: &[String], keep: &HashSet<&str>) -> Vec<String> {
    let mut out = saved.to_vec();
    let mut cursor: Option<usize> = None;
    let mut leading: Vec<String> = Vec::new();
    for id in stored {
        if let Some(at) = out.iter().position(|o| o == id) {
            let at = if cursor.is_none() && !leading.is_empty() {
                let n = leading.len();
                out.splice(at..at, leading.drain(..));
                at + n
            } else {
                at
            };
            cursor = Some(at);
        } else if keep.contains(id.as_str()) && !leading.contains(id) {
            match cursor {
                Some(c) => {
                    out.insert(c + 1, id.clone());
                    cursor = Some(c + 1);
                }
                None => leading.push(id.clone()),
            }
        }
    }
    out.extend(leading);
    out
}

/// Replace `old` with `new` in place, for a rename that moved the instance folder.
pub fn rename_in_order(order: &mut [String], old: &str, new: &str) {
    for id in order.iter_mut().filter(|id| id.as_str() == old) {
        *id = new.to_string();
    }
}

#[derive(Serialize, Deserialize)]
struct OrderFile {
    version: u32,
    ids: Vec<String>,
}

pub struct OrderStore {
    path: PathBuf,
}

impl OrderStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The stored order. A missing or damaged file is an empty order: there's nothing in it to keep,
    /// so saving over it is fine. `None` when the file is there but can't be read right now (another
    /// program holds it, say): show the default order and don't save over the player's.
    pub async fn load(&self) -> Option<Vec<String>> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => Some(
                serde_json::from_slice::<OrderFile>(&bytes)
                    .map(|f| f.ids)
                    .unwrap_or_default(),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Vec::new()),
            Err(_) => None,
        }
    }

    pub async fn save(&self, ids: &[String]) -> Result<()> {
        let bytes = serde_json::to_vec(&OrderFile {
            version: 1,
            ids: ids.to_vec(),
        })
        .map_err(|e| {
            LauncherError::disk(
                ErrorCode::DiskWriteFailed,
                "Couldn't save the instance order",
            )
            .with_param("item", "instance order")
            .with_detail(e)
        })?;
        write_atomic(&self.path, &bytes).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::{GameSpec, InstanceStore};

    async fn make(names: &[&str]) -> Vec<Instance> {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        let mut out = Vec::new();
        for n in names {
            let game = GameSpec {
                minecraft: "1.21.1".into(),
                loader: None,
            };
            out.push(store.create(n, game).await.unwrap());
        }
        out
    }

    fn ids(v: &[Instance]) -> Vec<&str> {
        v.iter().map(|i| i.id.as_str()).collect()
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[tokio::test]
    async fn empty_order_is_a_to_z_and_returns_all_ids() {
        let mut list = make(&["Charlie", "alpha", "Bravo"]).await;
        let out = apply_order(&mut list, &[]);
        assert_eq!(ids(&list), ["alpha", "bravo", "charlie"]);
        assert_eq!(out, s(&["alpha", "bravo", "charlie"]));
    }

    #[tokio::test]
    async fn unlisted_instances_come_first_then_the_stored_order() {
        let mut list = make(&["a", "b", "c"]).await;
        let out = apply_order(&mut list, &s(&["c", "a"]));
        assert_eq!(ids(&list), ["b", "c", "a"]);
        assert_eq!(out, s(&["b", "c", "a"]));
    }

    #[tokio::test]
    async fn stale_and_duplicate_ids_are_dropped() {
        let mut list = make(&["a", "b"]).await;
        let out = apply_order(&mut list, &s(&["gone", "b", "b", "a"]));
        assert_eq!(ids(&list), ["b", "a"]);
        assert_eq!(out, s(&["b", "a"]));
    }

    #[test]
    fn rename_replaces_in_place() {
        let mut order = s(&["a", "b", "c"]);
        rename_in_order(&mut order, "b", "bb");
        assert_eq!(order, s(&["a", "bb", "c"]));
        rename_in_order(&mut order, "missing", "x");
        assert_eq!(order, s(&["a", "bb", "c"]));
    }

    #[tokio::test]
    async fn store_round_trips_and_writes_the_exact_shape() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("instance-order.json");
        let store = OrderStore::new(&path);
        store.save(&s(&["b", "a"])).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"version":1,"ids":["b","a"]}"#
        );
        assert_eq!(store.load().await, Some(s(&["b", "a"])));
    }

    #[tokio::test]
    async fn missing_or_garbage_file_is_an_empty_order() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("instance-order.json");
        let store = OrderStore::new(&path);
        assert_eq!(store.load().await, Some(Vec::new()));
        for junk in ["{oops", r#"{"ids":"nope"}"#, "[1,2]", ""] {
            std::fs::write(&path, junk).unwrap();
            assert_eq!(store.load().await, Some(Vec::new()), "{junk}");
        }
    }

    #[tokio::test]
    async fn an_unreadable_file_is_no_answer() {
        let tmp = tempfile::tempdir().unwrap();
        // A folder where the file should be: it exists but can't be read as a file.
        let path = tmp.path().join("instance-order.json");
        std::fs::create_dir(&path).unwrap();
        assert_eq!(OrderStore::new(&path).load().await, None);
    }

    #[test]
    fn kept_ids_stay_beside_the_id_they_followed() {
        let keep: HashSet<&str> = ["x", "y"].into_iter().collect();
        // x followed a, so it goes back after a (behind any new id on top).
        assert_eq!(
            keep_in_place(
                &s(&["new", "a", "b", "c"]),
                &s(&["a", "x", "b", "c"]),
                &keep
            ),
            s(&["new", "a", "x", "b", "c"])
        );
        // Leading kept ids go before the first id that follows them.
        assert_eq!(
            keep_in_place(&s(&["new", "a", "b"]), &s(&["x", "y", "a", "b"]), &keep),
            s(&["new", "x", "y", "a", "b"])
        );
        // A new order from a drag: x keeps following a wherever a went.
        assert_eq!(
            keep_in_place(&s(&["c", "a", "b"]), &s(&["a", "x", "b", "c"]), &keep),
            s(&["c", "a", "x", "b"])
        );
        // Ids not kept are dropped; with nothing to follow, kept ids go last.
        assert_eq!(
            keep_in_place(&s(&["a"]), &s(&["gone", "x"]), &keep),
            s(&["a", "x"])
        );
        assert_eq!(
            keep_in_place(&s(&["a"]), &s(&["x", "x"]), &keep),
            s(&["a", "x"])
        );
    }
}
