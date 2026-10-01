use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

const SAMPLE_BYTES: usize = 8 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentMetadata {
    size: u64,
    last_write: Option<SystemTime>,
    is_text: bool,
}

impl AttachmentMetadata {
    fn matches(&self, metadata: &Metadata) -> bool {
        self.last_write.is_some() && self.same_version(metadata)
    }

    fn same_version(&self, metadata: &Metadata) -> bool {
        self.size == metadata.len() && self.last_write == metadata.modified().ok()
    }
}

pub fn deserialize_metadata<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, AttachmentMetadata>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

#[derive(Default)]
pub struct AttachmentMetadataCache {
    pub entries: BTreeMap<String, AttachmentMetadata>,
    pub dirty: bool,
}

impl AttachmentMetadataCache {
    pub fn prune(&mut self, root: &Path) {
        let previous_count = self.entries.len();
        self.entries.retain(|relative_path, entry| {
            crate::files::resolve_attachment_path(root, relative_path)
                .ok()
                .and_then(|path| fs::metadata(path).ok())
                .is_some_and(|metadata| metadata.is_file() && entry.matches(&metadata))
        });
        self.dirty |= self.entries.len() != previous_count;
    }

    pub fn read(&mut self, path: &Path, relative_path: &str, max_bytes: u64) -> Option<String> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => metadata,
            _ => {
                self.remove(relative_path);
                return None;
            }
        };
        let cached = self.entries.get(relative_path);
        let classification = cached
            .filter(|entry| entry.matches(&metadata))
            .map(|entry| entry.is_text);
        if cached.is_some() && classification.is_none() {
            self.remove(relative_path);
        }
        if metadata.len() > max_bytes {
            return None;
        }
        if classification == Some(false) {
            return None;
        }
        if has_binary_extension(path) {
            self.remember(relative_path, &metadata, false);
            return None;
        }
        let version = AttachmentMetadata {
            size: metadata.len(),
            last_write: metadata.modified().ok(),
            is_text: true,
        };
        let mut file = File::open(path).ok()?;
        if !version.same_version(&file.metadata().ok()?) {
            return None;
        }
        let mut bytes = if classification == Some(true) {
            Vec::new()
        } else {
            let sample = read_sample(&mut file, metadata.len())?;
            if !sample_is_text(&sample, metadata.len() > sample.len() as u64) {
                if version.same_version(&file.metadata().ok()?)
                    && version.same_version(&fs::metadata(path).ok()?)
                {
                    self.remember(relative_path, &metadata, false);
                }
                return None;
            }
            sample
        };
        let remaining = max_bytes
            .saturating_add(1)
            .saturating_sub(bytes.len() as u64);
        (&mut file).take(remaining).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > max_bytes
            || !version.same_version(&file.metadata().ok()?)
            || !version.same_version(&fs::metadata(path).ok()?)
        {
            self.remove(relative_path);
            return None;
        }
        let text = String::from_utf8(bytes).ok();
        self.remember(relative_path, &metadata, text.is_some());
        text
    }

    fn remember(&mut self, relative_path: &str, metadata: &Metadata, is_text: bool) {
        let entry = AttachmentMetadata {
            size: metadata.len(),
            last_write: metadata.modified().ok(),
            is_text,
        };
        if self.entries.get(relative_path) != Some(&entry) {
            self.entries.insert(relative_path.to_owned(), entry);
            self.dirty = true;
        }
    }

    fn remove(&mut self, relative_path: &str) {
        self.dirty |= self.entries.remove(relative_path).is_some();
    }
}

fn has_binary_extension(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "bin"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "obj"
            | "o"
            | "class"
            | "pyc"
            | "wasm"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "ppt"
            | "pptx"
            | "odt"
            | "ods"
            | "odp"
            | "zip"
            | "gz"
            | "bz2"
            | "xz"
            | "7z"
            | "rar"
            | "tar"
            | "jar"
            | "iso"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "bmp"
            | "ico"
            | "tif"
            | "tiff"
            | "heic"
            | "mp3"
            | "wav"
            | "ogg"
            | "flac"
            | "m4a"
            | "mp4"
            | "mkv"
            | "mov"
            | "avi"
            | "webm"
            | "woff"
            | "woff2"
            | "ttf"
            | "otf"
            | "sqlite"
            | "db"
    )
}

fn read_sample(reader: &mut impl Read, file_size: u64) -> Option<Vec<u8>> {
    let mut sample = vec![0; file_size.min(SAMPLE_BYTES as u64) as usize];
    reader.read_exact(&mut sample).ok()?;
    Some(sample)
}

fn sample_is_text(sample: &[u8], has_more: bool) -> bool {
    let text = match std::str::from_utf8(sample) {
        Ok(text) => text,
        Err(error) if has_more && error.error_len().is_none() => {
            let Ok(text) = std::str::from_utf8(&sample[..error.valid_up_to()]) else {
                return false;
            };
            text
        }
        Err(_) => return false,
    };
    !text
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn persisted_metadata_reuses_classification_and_invalidates_changed_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unknown");
        fs::write(&path, [0, 1, 2]).unwrap();
        let mut cache = AttachmentMetadataCache::default();
        assert!(cache.read(&path, "unknown", 100).is_none());
        let json = serde_json::to_string(&cache.entries).unwrap();
        let mut restored = AttachmentMetadataCache {
            entries: serde_json::from_str(&json).unwrap(),
            dirty: false,
        };
        assert!(restored.read(&path, "unknown", 100).is_none());
        assert!(!restored.dirty);
        fs::write(&path, "yes").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(123456))
            .unwrap();
        assert_eq!(restored.read(&path, "unknown", 100).as_deref(), Some("yes"));
        assert!(restored.dirty);
        assert!(restored.read(&path, "unknown", 2).is_none());
        let json = serde_json::to_value(&restored.entries).unwrap();
        assert_eq!(json["unknown"].as_object().unwrap().len(), 3);
        assert_eq!(json["unknown"]["isText"], true);
        assert_eq!(json["unknown"]["size"], 3);
        fs::remove_file(&path).unwrap();
        assert!(restored.read(&path, "unknown", 100).is_none());
        assert!(restored.entries.is_empty());
    }

    #[test]
    fn binary_extensions_and_samples_are_rejected_before_a_full_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("document.PDF");
        fs::write(&path, "ASCII PDF header").unwrap();
        let mut cache = AttachmentMetadataCache::default();
        assert!(cache.read(&path, "document.PDF", 100).is_none());
        assert!(!cache.entries["document.PDF"].is_text);
        let mut reader = Cursor::new(vec![0; SAMPLE_BYTES * 4]);
        let sample = read_sample(&mut reader, (SAMPLE_BYTES * 4) as u64).unwrap();
        assert_eq!(reader.position(), SAMPLE_BYTES as u64);
        assert!(!sample_is_text(&sample, true));
    }

    #[test]
    fn sample_boundary_does_not_reject_a_split_utf8_character() {
        assert!(sample_is_text(&[b'a', 0xe2, 0x82], true));
        assert!(!sample_is_text(&[b'a', 0xe2, 0x82], false));
        assert!(!sample_is_text(&[b'a', 0xff], true));
    }
}
