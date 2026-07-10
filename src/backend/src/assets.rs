use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::collections::HashSet;
use std::fs;

use crate::store::{flatten_pages, Store};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn save_image(store: &Store, page_id: &str, data_b64: &str, ext: &str) -> Result<String, String> {
    if store.find_page(page_id).is_none() {
        return Err("page not found".into());
    }
    let ext = match ext.to_lowercase().as_str() {
        e @ ("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") => e.to_string(),
        _ => "png".to_string(),
    };
    let bytes = STANDARD.decode(data_b64).map_err(err)?;
    let dir = store.root.join("assets").join(page_id);
    fs::create_dir_all(&dir).map_err(err)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%3f");
    let mut name = format!("img-{stamp}.{ext}");
    let mut counter = 1;
    while dir.join(&name).exists() {
        name = format!("img-{stamp}-{counter}.{ext}");
        counter += 1;
    }
    fs::write(dir.join(&name), bytes).map_err(err)?;
    Ok(format!("assets/{page_id}/{name}"))
}

pub fn prune(store: &Store) -> Result<usize, String> {
    let assets_dir = store.root.join("assets");
    if !assets_dir.is_dir() {
        return Ok(0);
    }
    let page_ids: HashSet<String> = flatten_pages(&store.notebook)
        .into_iter()
        .map(|(_, p)| p.id.clone())
        .collect();
    let mut removed = 0;

    for entry in fs::read_dir(&assets_dir).map_err(err)? {
        let entry = entry.map_err(err)?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let page_id = entry.file_name().to_string_lossy().to_string();
        if !page_ids.contains(&page_id) {
            removed += count_files(&dir);
            let _ = fs::remove_dir_all(&dir);
            continue;
        }
        let content = store.read_page(&page_id).unwrap_or_default();
        let mut left = 0;
        for file in fs::read_dir(&dir).map_err(err)? {
            let file = file.map_err(err)?;
            let name = file.file_name().to_string_lossy().to_string();
            if content.contains(&format!("assets/{page_id}/{name}")) {
                left += 1;
            } else {
                let _ = fs::remove_file(file.path());
                removed += 1;
            }
        }
        if left == 0 {
            let _ = fs::remove_dir(&dir);
        }
    }
    Ok(removed)
}

fn count_files(dir: &std::path::Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).count())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use tempfile::tempdir;

    const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    #[test]
    fn save_image_writes_file_under_page_assets() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        let rel = save_image(&store, &page.id, TINY_PNG_B64, "png").unwrap();
        assert!(rel.starts_with(&format!("assets/{}/", page.id)));
        assert!(dir.path().join(&rel).exists());
    }

    #[test]
    fn prune_removes_orphans_and_keeps_referenced() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();

        let kept = save_image(&store, &page.id, TINY_PNG_B64, "png").unwrap();
        let orphan = save_image(&store, &page.id, TINY_PNG_B64, "png").unwrap();
        store
            .write_page(&page.id, &format!("# Pics\n\n![shot]({kept})\n"))
            .unwrap();

        // a whole directory for a page that no longer exists
        let ghost_dir = dir.path().join("assets").join("no-such-page");
        std::fs::create_dir_all(&ghost_dir).unwrap();
        std::fs::write(ghost_dir.join("stray.png"), b"x").unwrap();

        let removed = prune(&store).unwrap();
        assert_eq!(removed, 2);
        assert!(dir.path().join(&kept).exists());
        assert!(!dir.path().join(&orphan).exists());
        assert!(!ghost_dir.exists());
    }

    #[test]
    fn save_image_rejects_unknown_page() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert!(save_image(&store, "nope", TINY_PNG_B64, "png").is_err());
    }
}
