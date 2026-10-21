//! Image decoding and the preview cache.
//!
//! Decoding runs off the UI thread through `spawn_blocking` in the actor. This
//! module owns only decoded RGBA data: the core never depends on iced, and the
//! renderer turns the bytes into a texture handle once per generation.

use std::{
    collections::{HashMap, VecDeque},
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// Decoded RGBA image, already downsampled to the preview width.
#[derive(Clone)]
pub struct PreviewImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

impl PreviewImage {
    pub fn pixels(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

impl fmt::Debug for PreviewImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreviewImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}

/// Identity of one cached decode: file, modification time, and target width.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    path: PathBuf,
    modified: Option<u128>,
    width: u32,
}

impl CacheKey {
    pub fn new(path: &Path, modified: Option<SystemTime>, width: u32) -> Self {
        let modified = modified
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos());
        Self {
            path: path.to_path_buf(),
            modified,
            width,
        }
    }
}

/// Bounded LRU for decoded images, limited by total pixels rather than a count.
pub struct PreviewCache {
    entries: HashMap<CacheKey, Arc<PreviewImage>>,
    order: VecDeque<CacheKey>,
    pixels: usize,
    budget: usize,
}

impl PreviewCache {
    pub fn new() -> Self {
        Self::with_budget(32_000_000)
    }

    fn with_budget(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            pixels: 0,
            budget,
        }
    }

    pub fn get(&mut self, key: &CacheKey) -> Option<Arc<PreviewImage>> {
        let image = self.entries.get(key)?.clone();
        if let Some(index) = self.order.iter().position(|entry| entry == key) {
            self.order.remove(index);
        }
        self.order.push_back(key.clone());
        Some(image)
    }

    pub fn insert(&mut self, key: CacheKey, image: Arc<PreviewImage>) {
        if image.pixels() > self.budget {
            return;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.pixels = self.pixels.saturating_sub(previous.pixels());
            if let Some(index) = self.order.iter().position(|entry| entry == &key) {
                self.order.remove(index);
            }
        }

        while self.pixels + image.pixels() > self.budget {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest) {
                self.pixels = self.pixels.saturating_sub(evicted.pixels());
            }
        }

        self.pixels += image.pixels();
        self.order.push_back(key.clone());
        self.entries.insert(key, image);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Default for PreviewCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode and downsample an image without touching the UI thread.
pub fn decode_image(path: &Path, target_width: u32) -> Result<PreviewImage, String> {
    let reader = image::ImageReader::open(path)
        .map_err(|error| format!("open failed: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("format detection failed: {error}"))?;
    let decoded = reader
        .decode()
        .map_err(|error| format!("decode failed: {error}"))?;

    let (source_width, source_height) = (decoded.width(), decoded.height());
    let target_width = target_width.clamp(64, 2048);

    let (width, height) = if source_width > target_width {
        let scale = target_width as f64 / source_width as f64;
        (
            target_width,
            ((source_height as f64 * scale).round() as u32).max(1),
        )
    } else {
        (source_width, source_height)
    };

    let rgba = decoded.to_rgba8();
    let resized = if width != source_width || height != source_height {
        image::imageops::thumbnail(&rgba, width, height)
    } else {
        rgba
    };
    let bytes: Arc<[u8]> = Arc::from(resized.into_raw().into_boxed_slice());

    Ok(PreviewImage {
        width,
        height,
        rgba: bytes,
    })
}

#[cfg(test)]
mod tests {
    use std::env;

    use image::{Rgba, RgbaImage};

    use super::*;

    fn temp_image(name: &str, width: u32, height: u32) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "ferriso-preview-{name}-{}-{width}x{height}.png",
            std::process::id()
        ));
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([(x % 255) as u8, (y % 255) as u8, 90, 255]);
        }
        image.save(&path).expect("save fixture");
        path
    }

    #[test]
    fn decode_downsamples_to_the_target_width() {
        let path = temp_image("decode", 400, 200);
        let image = decode_image(&path, 100).expect("decode");
        assert_eq!(image.width, 100);
        assert_eq!(image.height, 50);
        assert_eq!(image.rgba.len(), 100 * 50 * 4);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cache_evicts_by_pixel_budget() {
        let mut cache = PreviewCache::with_budget(5);
        let key = |index| CacheKey::new(Path::new("fixture.png"), None, index);
        let image = |pixels| {
            Arc::new(PreviewImage {
                width: pixels,
                height: 1,
                rgba: Arc::from(vec![0; pixels as usize * 4]),
            })
        };

        cache.insert(key(1), image(2));
        cache.insert(key(2), image(3));
        assert_eq!(cache.len(), 2);
        assert!(cache.get(&key(1)).is_some());

        cache.insert(key(3), image(5));
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&key(1)).is_none());
        assert!(cache.get(&key(2)).is_none());
        assert!(cache.get(&key(3)).is_some());
    }
}
