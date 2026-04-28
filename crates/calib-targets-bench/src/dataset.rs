//! `datasets.toml` parsing.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ImageKind {
    /// Lives in `testdata/`, baseline committed.
    Public,
    /// Lives in `privatedata/`, baseline gitignored.
    Private,
}

/// Description of a horizontally-tiled stitched composite (e.g. 6 × 720 ×
/// 540 sub-snaps glued into a 4320 × 540 PNG).
#[derive(Clone, Debug, Deserialize)]
pub struct Stitched {
    pub count: u32,
    pub snap_width: u32,
    pub snap_height: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatasetEntry {
    pub path: String,
    pub kind: ImageKind,
    #[serde(default)]
    pub note: String,
    /// Bilinear upscale factor applied to each sub-snap before detection.
    /// Defaults to 1 (no upscale).
    #[serde(default = "default_upscale")]
    pub upscale: u32,
    /// `Some(spec)` when the on-disk file is a stitched composite of
    /// `count` sub-snaps. `None` for plain single-image entries.
    #[serde(default)]
    pub stitched: Option<Stitched>,
}

fn default_upscale() -> u32 {
    1
}

impl DatasetEntry {
    /// Number of logical sub-snaps this entry produces. Always ≥ 1.
    pub fn snap_count(&self) -> u32 {
        self.stitched.as_ref().map(|s| s.count).unwrap_or(1)
    }

    /// Logical label per sub-snap. For stitched entries: `path#k`; for
    /// single-image entries: just `path`. Used as the baseline JSON key.
    pub fn snap_label(&self, k: u32) -> String {
        if self.stitched.is_some() {
            format!("{}#{k}", self.path)
        } else {
            self.path.clone()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Dataset {
    #[serde(rename = "image", default)]
    pub images: Vec<DatasetEntry>,
}

impl Dataset {
    /// Load `crates/calib-targets-bench/datasets.toml`.
    pub fn load_default() -> Result<Self, std::io::Error> {
        let path = crate::workspace_root().join("crates/calib-targets-bench/datasets.toml");
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self, std::io::Error> {
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| std::io::Error::other(e.to_string()))
    }

    pub fn iter_kind(&self, kind: Option<ImageKind>) -> impl Iterator<Item = &DatasetEntry> {
        self.images
            .iter()
            .filter(move |e| kind.map(|k| e.kind == k).unwrap_or(true))
    }

    pub fn find(&self, image_path: &str) -> Option<&DatasetEntry> {
        self.images.iter().find(|e| e.path == image_path)
    }
}

impl DatasetEntry {
    /// Absolute path to the image on disk, anchored at the workspace root.
    pub fn absolute(&self) -> PathBuf {
        crate::workspace_root().join(&self.path)
    }
}
