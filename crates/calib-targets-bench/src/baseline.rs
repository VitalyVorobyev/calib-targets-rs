//! Baseline file I/O.
//!
//! One JSON file per dataset partition (`baselines/chessboard.json` for
//! testdata, `privatedata/baselines/chessboard.json` for privatedata).
//! Each maps image-relative path → expected detector output.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::dataset::ImageKind;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BaselineCorner {
    pub i: i32,
    pub j: i32,
    pub x: f32,
    pub y: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    pub score: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaselineImage {
    pub labelled_count: usize,
    pub cell_size_px: f32,
    /// Sorted by (j, i) for stable JSON output.
    pub corners: Vec<BaselineCorner>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Baseline {
    pub schema: u32,
    pub detector: String,
    pub config_id: String,
    pub generated_at: String,
    /// `BTreeMap` so the JSON is sorted by image path → stable diffs.
    pub images: BTreeMap<String, BaselineImage>,
}

impl Baseline {
    /// Empty baseline with the standard header.
    pub fn empty() -> Self {
        Self {
            schema: crate::SCHEMA_VERSION,
            detector: "chessboard".into(),
            config_id: "default".into(),
            generated_at: now_iso8601(),
            images: BTreeMap::new(),
        }
    }

    pub fn path_for(kind: ImageKind) -> PathBuf {
        let root = crate::workspace_root();
        match kind {
            ImageKind::Public => root.join("crates/calib-targets-bench/baselines/chessboard.json"),
            ImageKind::Private => root.join("privatedata/baselines/chessboard.json"),
        }
    }

    pub fn load(kind: ImageKind) -> Result<Self, std::io::Error> {
        let path = Self::path_for(kind);
        Self::load_from(&path)
    }

    pub fn load_or_empty(kind: ImageKind) -> Self {
        Self::load(kind).unwrap_or_else(|_| Self::empty())
    }

    pub fn load_from(path: &Path) -> Result<Self, std::io::Error> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|e| std::io::Error::other(e.to_string()))
    }

    pub fn save(&self, kind: ImageKind) -> Result<PathBuf, std::io::Error> {
        let path = Self::path_for(kind);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text =
            serde_json::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(&path, text)?;
        Ok(path)
    }
}

fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Cheap formatter — no chrono dep needed for a timestamp on a JSON file.
    format!("unix:{secs}")
}
