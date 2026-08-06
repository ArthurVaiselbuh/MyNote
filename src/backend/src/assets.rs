use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::store::{self, assets_rel, flatten_pages, Store};

use crate::err;

/// ext → MIME for the formats MyNote itself stores.
const WRITTEN_IMAGE_TYPES: [(&str, &str); 6] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
];

/// Served as well, but never written by MyNote — an `.svg` can only reach a
/// notebook by hand.
const SERVED_ONLY_TYPES: [(&str, &str); 1] = [("svg", "image/svg+xml")];

pub fn content_type(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    WRITTEN_IMAGE_TYPES
        .iter()
        .chain(SERVED_ONLY_TYPES.iter())
        .find(|(candidate, _)| ext.eq_ignore_ascii_case(candidate))
        .map_or("application/octet-stream", |(_, mime)| *mime)
}

pub fn written_ext(ext: &str) -> Option<&'static str> {
    WRITTEN_IMAGE_TYPES
        .iter()
        .find(|(candidate, _)| ext.eq_ignore_ascii_case(candidate))
        .map(|(ext, _)| *ext)
}

pub fn ext_for_mime(mime: &str) -> &'static str {
    let mime = mime.trim();
    // "image/jpg" is a nonstandard spelling that shows up in real exports
    if mime.eq_ignore_ascii_case("image/jpg") {
        return "jpg";
    }
    WRITTEN_IMAGE_TYPES
        .iter()
        .find(|(_, candidate)| mime.eq_ignore_ascii_case(candidate))
        .map_or("png", |(ext, _)| *ext)
}

pub fn save_image(store: &Store, page_id: &str, data_b64: &str, ext: &str) -> Result<String, String> {
    if store.find_page(page_id).is_none() {
        return Err("page not found".into());
    }
    let ext = written_ext(ext).unwrap_or("png");
    let bytes = STANDARD.decode(data_b64).map_err(err)?;
    let dir = store.assets_dir(page_id);
    fs::create_dir_all(&dir).map_err(err)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%3f");
    let mut name = format!("img-{stamp}.{ext}");
    let mut counter = 1;
    while dir.join(&name).exists() {
        name = format!("img-{stamp}-{counter}.{ext}");
        counter += 1;
    }
    fs::write(dir.join(&name), bytes).map_err(err)?;
    Ok(assets_rel(page_id, &name))
}

pub fn prune(store: &Store) -> Result<usize, String> {
    let assets_dir = store.root.join(store::ASSETS_DIR);
    if !assets_dir.is_dir() {
        return Ok(0);
    }
    let live_pages: HashSet<&str> = flatten_pages(&store.notebook)
        .iter()
        .map(|(_, page)| page.id.as_str())
        .collect();
    let mut removed = 0;

    for entry in fs::read_dir(&assets_dir).map_err(err)? {
        let entry = entry.map_err(err)?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let page_id = entry.file_name().to_string_lossy().into_owned();
        removed += if live_pages.contains(page_id.as_str()) {
            drop_unreferenced(store, &dir, &page_id)?
        } else {
            drop_whole_dir(&dir)
        };
    }
    Ok(removed)
}

fn drop_unreferenced(store: &Store, dir: &Path, page_id: &str) -> Result<usize, String> {
    let content = match store.read_page(page_id) {
        Ok(content) => content,
        Err(e) => {
            log::warn!("skipping asset prune for page {page_id}: {e}");
            return Ok(0);
        }
    };
    let mut removed = 0;
    let mut kept = 0;
    for file in fs::read_dir(dir).map_err(err)? {
        let file = file.map_err(err)?;
        let name = file.file_name().to_string_lossy().into_owned();
        if content.contains(&assets_rel(page_id, &name)) {
            kept += 1;
        } else {
            let _ = fs::remove_file(file.path());
            removed += 1;
        }
    }
    if kept == 0 {
        let _ = fs::remove_dir(dir);
    }
    Ok(removed)
}

fn drop_whole_dir(dir: &Path) -> usize {
    let removed = count_files(dir);
    let _ = fs::remove_dir_all(dir);
    removed
}

fn count_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).count())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    #[test]
    fn save_image_writes_file_under_page_assets() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        let rel = save_image(&store, &page.id, TINY_PNG_B64, "png").unwrap();
        assert!(rel.starts_with(&store::assets_rel_dir(&page.id)));
        assert!(dir.path().join(&rel).exists());
    }

    #[test]
    fn content_type_ignores_extension_case() {
        assert_eq!(content_type(Path::new("a.PNG")), "image/png");
        assert_eq!(content_type(Path::new("a.JpEg")), "image/jpeg");
        assert_eq!(content_type(Path::new("a.svg")), "image/svg+xml");
        assert_eq!(content_type(Path::new("a.zip")), "application/octet-stream");
        assert_eq!(content_type(Path::new("noext")), "application/octet-stream");
    }

    #[test]
    fn only_written_formats_resolve_to_themselves() {
        assert_eq!(written_ext("JPEG"), Some("jpeg"));
        assert_eq!(written_ext("svg"), None);
        assert_eq!(ext_for_mime(" image/jpg "), "jpg");
        assert_eq!(ext_for_mime("image/svg+xml"), "png");
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
    fn prune_keeps_assets_when_page_content_is_unreadable() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        let img = save_image(&store, &page.id, TINY_PNG_B64, "png").unwrap();

        // page still in the tree, but its .md is gone (transient read failure):
        // prune must not treat that as "no references" and delete the images.
        std::fs::remove_file(store.page_path(&page.id)).unwrap();

        let removed = prune(&store).unwrap();
        assert_eq!(removed, 0);
        assert!(dir.path().join(&img).exists());
    }

    #[test]
    fn save_image_rejects_unknown_page() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert!(save_image(&store, "nope", TINY_PNG_B64, "png").is_err());
    }
}
