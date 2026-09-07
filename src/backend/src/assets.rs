use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use percent_encoding::percent_decode_str;
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

pub fn referenced_assets(content: &str) -> Vec<(String, String)> {
    crate::files::scan_refs(content, store::ASSETS_DIR)
        .into_iter()
        .map(|(page_id, raw)| {
            let name = percent_decode_str(&raw).decode_utf8_lossy().into_owned();
            (page_id, name)
        })
        .collect()
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
    let mut referenced_anywhere: HashSet<(String, String)> = HashSet::new();
    let mut live_content = Vec::new();
    for &page_id in &live_pages {
        let content = match store.read_page(page_id) {
            Ok(content) => content,
            Err(e) => {
                log::warn!("skipping asset prune because page {page_id} could not be read: {e}");
                return Ok(0);
            }
        };
        referenced_anywhere.extend(referenced_assets(&content));
        live_content.push(content);
    }
    let mut removed = 0;

    for entry in fs::read_dir(&assets_dir).map_err(err)? {
        let entry = entry.map_err(err)?;
        let dir = entry.path();
        if !fs::symlink_metadata(&dir)
            .map_err(err)?
            .file_type()
            .is_dir()
        {
            continue;
        }
        let page_id = entry.file_name().to_string_lossy().into_owned();
        if live_pages.contains(page_id.as_str()) {
            removed += drop_unreferenced(&dir, &page_id, &referenced_anywhere, &live_content)?;
        }
    }
    Ok(removed)
}

pub(crate) fn drop_unreferenced(
    dir: &Path,
    page_id: &str,
    referenced_anywhere: &HashSet<(String, String)>,
    live_content: &[String],
) -> Result<usize, String> {
    let mut removed = 0;
    let mut kept = 0;
    for file in fs::read_dir(dir).map_err(err)? {
        let file = file.map_err(err)?;
        let name = file.file_name().to_string_lossy().into_owned();
        if crate::files::retains_entry_or_content(
            referenced_anywhere,
            live_content,
            store::ASSETS_DIR,
            page_id,
            &name,
        ) {
            kept += 1;
        } else {
            fs::remove_file(file.path()).map_err(err)?;
            removed += 1;
        }
    }
    if kept == 0 {
        let _ = fs::remove_dir(dir);
    }
    Ok(removed)
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
    fn referenced_assets_reads_names_out_of_a_page_body() {
        let id = "11111111-1111-1111-1111-111111111111";
        let other = "22222222-2222-2222-2222-222222222222";
        let body = format!(
            "# T\n\n![a](assets/{id}/img-1.png)\n![b](assets/{id}/img-2.jpg){{width=420}}\n\
             ![c](assets/{other}/shared.gif)\n![a again](assets/{id}/img-1.png)\n"
        );
        let found = referenced_assets(&body);
        assert_eq!(
            found,
            vec![
                (id.to_string(), "img-1.png".to_string()),
                (id.to_string(), "img-2.jpg".to_string()),
                (other.to_string(), "shared.gif".to_string()),
                (id.to_string(), "img-1.png".to_string()),
            ]
        );
    }

    #[test]
    fn referenced_assets_skips_paths_that_are_not_page_assets() {
        let id = "11111111-1111-1111-1111-111111111111";
        assert!(referenced_assets("![x](assets/not-a-page-id/img.png)").is_empty());
        assert!(referenced_assets("![x](assets/img.png)").is_empty());
        assert!(referenced_assets(&format!("![x](assets/{id}/)")).is_empty());
        assert!(referenced_assets("plain text with no links at all").is_empty());
    }

    #[test]
    fn prune_removes_unreferenced_images_of_live_pages_only() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();

        let kept = save_image(&store, &page.id, TINY_PNG_B64, "png").unwrap();
        let orphan = save_image(&store, &page.id, TINY_PNG_B64, "png").unwrap();
        store
            .write_page(&page.id, &format!("# Pics\n\n![shot]({kept})\n"))
            .unwrap();

        let ghost_dir = dir
            .path()
            .join("assets")
            .join("22222222-2222-2222-2222-222222222222");
        std::fs::create_dir_all(&ghost_dir).unwrap();
        std::fs::write(ghost_dir.join("stray.png"), b"x").unwrap();

        let removed = prune(&store).unwrap();
        assert_eq!(removed, 1);
        assert!(dir.path().join(&kept).exists());
        assert!(!dir.path().join(&orphan).exists());
        assert!(ghost_dir.exists());
    }

    #[test]
    fn prune_leaves_a_deleted_pages_images_for_close_to_handle() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        save_image(&store, &page.id, TINY_PNG_B64, "png").unwrap();

        store.delete_page(&page.id).unwrap();

        assert_eq!(prune(&store).unwrap(), 0);
        assert!(store.assets_dir(&page.id).is_dir());
    }

    #[test]
    fn prune_does_not_follow_an_asset_directory_symlink() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("outside.png"), b"outside").unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let section_id = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section_id, None, None).unwrap();
        let assets = store.assets_dir(&page.id);
        fs::create_dir_all(assets.parent().unwrap()).unwrap();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(outside.path(), &assets).is_ok();
        #[cfg(not(windows))]
        let linked = std::os::unix::fs::symlink(outside.path(), &assets).is_ok();
        if !linked {
            return;
        }

        prune(&store).unwrap();

        assert!(outside.path().join("outside.png").exists());
        assert!(fs::symlink_metadata(&assets)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[test]
    fn prune_keeps_an_image_referenced_only_from_another_page() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let owner = store.create_page(&sid, None, None).unwrap();
        let borrower = store.create_page(&sid, None, None).unwrap();
        let shared = save_image(&store, &owner.id, TINY_PNG_B64, "png").unwrap();

        store.write_page(&owner.id, "# Owner\n").unwrap();
        store
            .write_page(&borrower.id, &format!("# Borrower\n\n![shared]({shared})\n"))
            .unwrap();

        assert_eq!(prune(&store).unwrap(), 0);
        assert!(dir.path().join(shared).exists());
    }

    #[test]
    fn prune_keeps_an_angle_wrapped_destination_with_spaces() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        let assets = store.assets_dir(&page.id);
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("hand drawn.png"), b"image").unwrap();
        store
            .write_page(
                &page.id,
                &format!("# Page\n\n![drawing](<assets/{}/hand drawn.png>)\n", page.id),
            )
            .unwrap();

        assert_eq!(prune(&store).unwrap(), 0);
        assert!(assets.join("hand drawn.png").exists());
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
