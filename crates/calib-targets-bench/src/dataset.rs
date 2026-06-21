//! `datasets.toml` parsing.
//!
//! Each `[[image]]` entry is either a single `path` or a directory `glob`
//! (e.g. `privatedata/130x130_puzzle/target_*.png`) that expands into one
//! entry per matched file at load time. Both forms share the same `kind`,
//! `upscale`, `stitched`, `dataset` (group), and `min_labelled` fields. Glob
//! expansion is how the full private datasets (every `target_*.png`, ~120
//! snaps each) become browsable/runnable without listing 20 files by hand.

use serde::Deserialize;
use std::cmp::Ordering;
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

/// A concrete, expanded image entry. Always carries a real on-disk `path`
/// (glob entries are expanded into one of these per matched file at load).
#[derive(Clone, Debug)]
pub struct DatasetEntry {
    pub path: String,
    pub kind: ImageKind,
    pub note: String,
    /// Bilinear upscale factor applied to each sub-snap before detection.
    /// Defaults to 1 (no upscale).
    pub upscale: u32,
    /// `Some(spec)` when the on-disk file is a stitched composite of
    /// `count` sub-snaps. `None` for plain single-image entries.
    pub stitched: Option<Stitched>,
    /// Dataset group name (explicit, or derived from the parent directory)
    /// used for per-dataset browsing and runs.
    pub dataset: String,
    /// Baseline-free low-recall floor: snaps with fewer labelled corners are
    /// flagged as a likely problem in the studio. `None` ⇒ only no-detection
    /// (zero labelled corners) is flagged.
    pub min_labelled: Option<u32>,
}

fn default_upscale() -> u32 {
    1
}

/// Raw `[[image]]` row as written in `datasets.toml`. Either `path` (single
/// file) or `glob` (directory wildcard) must be set.
#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(default)]
    path: Option<String>,
    /// Indexed-frame wildcard `<prefix>*<suffix>` where `*` matches a frame
    /// index (one or more digits), e.g. `.../target_*.png`. Expands to one
    /// entry per matched file at load.
    #[serde(default)]
    glob: Option<String>,
    kind: ImageKind,
    #[serde(default)]
    note: String,
    #[serde(default = "default_upscale")]
    upscale: u32,
    #[serde(default)]
    stitched: Option<Stitched>,
    #[serde(default)]
    dataset: Option<String>,
    #[serde(default)]
    min_labelled: Option<u32>,
}

impl RawEntry {
    /// Build a concrete entry for one resolved `path`, sharing this row's
    /// shape and deriving the group name when not given explicitly.
    fn to_entry(&self, path: String) -> DatasetEntry {
        let dataset = self.dataset.clone().unwrap_or_else(|| derive_group(&path));
        DatasetEntry {
            path,
            kind: self.kind,
            note: self.note.clone(),
            upscale: self.upscale,
            stitched: self.stitched.clone(),
            dataset,
            min_labelled: self.min_labelled,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawDataset {
    #[serde(rename = "image", default)]
    images: Vec<RawEntry>,
}

/// Dataset group name from a workspace-relative path: the immediate parent
/// directory's name (`privatedata/130x130_puzzle/target_3.png` →
/// `130x130_puzzle`, `testdata/mid.png` → `testdata`).
fn derive_group(path: &str) -> String {
    Path::new(path)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

/// Natural (numeric-aware) string comparison so `target_2` precedes
/// `target_10`. Compares digit runs by value, everything else byte-wise.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.as_bytes().iter().peekable();
    let mut bi = b.as_bytes().iter().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&ca), Some(&cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let va = take_number(&mut ai);
                    let vb = take_number(&mut bi);
                    match va.cmp(&vb) {
                        Ordering::Equal => continue,
                        o => return o,
                    }
                } else {
                    match ca.cmp(&cb) {
                        Ordering::Equal => {
                            ai.next();
                            bi.next();
                        }
                        o => return o,
                    }
                }
            }
        }
    }
}

/// Consume a leading run of ASCII digits and return its value (saturating).
fn take_number<'a>(it: &mut std::iter::Peekable<impl Iterator<Item = &'a u8>>) -> u64 {
    let mut n: u64 = 0;
    while let Some(&&c) = it.peek() {
        if !c.is_ascii_digit() {
            break;
        }
        n = n.saturating_mul(10).saturating_add(u64::from(c - b'0'));
        it.next();
    }
    n
}

/// Match an indexed-frame `pattern` (`<prefix>*<suffix>`) against files in
/// `abs_dir`, returning matched file names in natural order. The single `*`
/// matches a **frame index — one or more ASCII digits** — so
/// `target_*.png` selects `target_0.png … target_19.png` but not sibling
/// debug artifacts like `target_0_xfeat_overlay.png`. A missing directory or
/// no matches yields an empty list (never an error — `privatedata/` is absent
/// on CI). Pure over `abs_dir` so it is unit-testable without the workspace
/// root.
fn matching_names(abs_dir: &Path, pattern: &str) -> Vec<String> {
    let Some(star) = pattern.find('*') else {
        return Vec::new();
    };
    let (prefix, suffix) = (&pattern[..star], &pattern[star + 1..]);
    let Ok(read_dir) = std::fs::read_dir(abs_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read_dir
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            // Bounds: a non-empty index requires strictly more than the fixed
            // prefix+suffix length, which also keeps the slice below in range.
            n.len() > prefix.len() + suffix.len()
                && n.starts_with(prefix)
                && n.ends_with(suffix)
                && n[prefix.len()..n.len() - suffix.len()]
                    .bytes()
                    .all(|b| b.is_ascii_digit())
        })
        .collect();
    names.sort_by(|a, b| natural_cmp(a, b));
    names
}

/// Expand a workspace-relative `glob` (`dir/<prefix>*<suffix>`) into the
/// matched workspace-relative paths, in natural order.
fn expand_glob(glob: &str) -> Vec<String> {
    let (dir, pat) = glob.rsplit_once('/').unwrap_or((".", glob));
    let abs_dir = crate::workspace_root().join(dir);
    matching_names(&abs_dir, pat)
        .into_iter()
        .map(|name| format!("{dir}/{name}"))
        .collect()
}

impl DatasetEntry {
    /// A single, non-stitched entry for `path` (used to diagnose a file that
    /// is not in the manifest). Group is derived from the parent directory.
    pub fn single(path: String, kind: ImageKind) -> Self {
        let dataset = derive_group(&path);
        Self {
            path,
            kind,
            note: String::new(),
            upscale: 1,
            stitched: None,
            dataset,
            min_labelled: None,
        }
    }

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

    /// Absolute path to the image on disk, anchored at the workspace root.
    pub fn absolute(&self) -> PathBuf {
        crate::workspace_root().join(&self.path)
    }
}

#[derive(Debug)]
pub struct Dataset {
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
        let raw: RawDataset =
            toml::from_str(&text).map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut images = Vec::new();
        for entry in &raw.images {
            match (&entry.glob, &entry.path) {
                (Some(glob), _) => {
                    for resolved in expand_glob(glob) {
                        images.push(entry.to_entry(resolved));
                    }
                }
                (None, Some(path)) => images.push(entry.to_entry(path.clone())),
                (None, None) => {
                    return Err(std::io::Error::other(
                        "dataset entry must set either `path` or `glob`",
                    ));
                }
            }
        }
        Ok(Dataset { images })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_names_natural_sorts_and_handles_missing() {
        // Missing directory → empty, never an error.
        assert!(matching_names(Path::new("/no/such/calib/dir"), "target_*.png").is_empty());

        let dir = std::env::temp_dir().join(format!("calib_ds_glob_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in [10u32, 2, 1, 0] {
            std::fs::write(dir.join(format!("target_{n}.png")), b"x").unwrap();
        }
        // All excluded: wrong prefix, wrong suffix, and a non-numeric index
        // (a debug overlay) that must not be mistaken for a frame.
        std::fs::write(dir.join("laser_0.png"), b"x").unwrap();
        std::fs::write(dir.join("poses.json"), b"x").unwrap();
        std::fs::write(dir.join("target_0_xfeat_overlay.png"), b"x").unwrap();
        std::fs::write(dir.join("target_.png"), b"x").unwrap();

        let names = matching_names(&dir, "target_*.png");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(
            names,
            vec![
                "target_0.png",
                "target_1.png",
                "target_2.png",
                "target_10.png"
            ]
        );
    }

    #[test]
    fn derive_group_uses_parent_dir() {
        assert_eq!(
            derive_group("privatedata/130x130_puzzle/target_3.png"),
            "130x130_puzzle"
        );
        assert_eq!(derive_group("testdata/mid.png"), "testdata");
    }
}
