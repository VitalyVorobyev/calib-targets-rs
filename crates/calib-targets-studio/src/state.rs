//! Shared server state: the dataset manifest plus a small cache of encoded
//! fed-image PNGs.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use calib_targets_bench::dataset::Dataset;

use crate::jobs::SharedRuns;

/// Maximum number of encoded fed-image PNGs kept in memory. The current
/// manifest tops out at ~27 logical snaps of a few hundred KB each, so this
/// effectively caches the whole dataset while still bounding memory if the
/// manifest grows.
const IMAGE_CACHE_CAP: usize = 32;

/// Process-wide state shared by all request handlers.
pub struct StudioState {
    /// Parsed `datasets.toml` (loaded once at startup).
    pub dataset: Dataset,
    /// Registry of dataset runs (at most one live at a time).
    pub runs: SharedRuns,
    /// FIFO-evicted cache of encoded fed-image PNGs keyed by snap label.
    image_cache: Mutex<ImageCache>,
}

impl StudioState {
    /// Load the dataset manifest and initialise empty caches.
    pub fn load() -> Result<Self, std::io::Error> {
        let dataset = Dataset::load_default()?;
        Ok(Self {
            dataset,
            runs: SharedRuns::default(),
            image_cache: Mutex::new(ImageCache::default()),
        })
    }

    /// Cached encoded PNG for `label`, if present.
    pub fn cached_png(&self, label: &str) -> Option<Arc<Vec<u8>>> {
        self.image_cache
            .lock()
            .expect("image cache lock")
            .get(label)
    }

    /// Insert an encoded PNG into the cache, evicting the oldest entry past
    /// the cap.
    pub fn cache_png(&self, label: &str, png: Arc<Vec<u8>>) {
        self.image_cache
            .lock()
            .expect("image cache lock")
            .insert(label, png);
    }
}

#[derive(Default)]
struct ImageCache {
    map: HashMap<String, Arc<Vec<u8>>>,
    order: VecDeque<String>,
}

impl ImageCache {
    fn get(&self, label: &str) -> Option<Arc<Vec<u8>>> {
        self.map.get(label).cloned()
    }

    fn insert(&mut self, label: &str, png: Arc<Vec<u8>>) {
        if self.map.insert(label.to_string(), png).is_none() {
            self.order.push_back(label.to_string());
            while self.order.len() > IMAGE_CACHE_CAP {
                if let Some(oldest) = self.order.pop_front() {
                    self.map.remove(&oldest);
                }
            }
        }
    }
}
