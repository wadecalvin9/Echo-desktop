use std::collections::BTreeMap;

/// Bounded byte-range cache & read-ahead tracker inspired by Echo's `SabrFile.kt`.
/// Keeps already-downloaded chunks in memory up to `max_bytes` so seeks within the buffered
/// window are instant and dropped connections do not interrupt playback.
pub struct ChunkBuffer {
    chunks: BTreeMap<u64, Vec<u8>>,
    total_bytes: usize,
    max_bytes: usize,
    pub content_length: Option<u64>,
}

impl ChunkBuffer {
    pub fn new(content_length: Option<u64>, max_bytes: usize) -> Self {
        Self {
            chunks: BTreeMap::new(),
            total_bytes: 0,
            max_bytes: max_bytes.max(512 * 1024),
            content_length,
        }
    }

    pub fn insert(&mut self, offset: u64, data: Vec<u8>, protect_offset: u64) {
        if data.is_empty() {
            return;
        }
        if let Some(old) = self.chunks.insert(offset, data) {
            self.total_bytes = self.total_bytes.saturating_sub(old.len());
        }
        self.total_bytes += self.chunks[&offset].len();
        self.evict_if_needed(protect_offset);
    }

    pub fn read_at(&self, offset: u64, out: &mut [u8]) -> usize {
        if out.is_empty() {
            return 0;
        }
        // Find chunk starting at or before `offset`.
        let mut cursor = offset;
        let mut written = 0usize;

        while written < out.len() {
            let found = self
                .chunks
                .range(..=cursor)
                .next_back()
                .and_then(|(&start, buf)| {
                    let end = start + buf.len() as u64;
                    (cursor < end).then_some((start, buf))
                });
            let Some((start, buf)) = found else {
                break;
            };
            let local_off = (cursor - start) as usize;
            let avail = buf.len() - local_off;
            let take = avail.min(out.len() - written);
            out[written..written + take].copy_from_slice(&buf[local_off..local_off + take]);
            written += take;
            cursor += take as u64;
        }

        written
    }

    pub fn has_byte(&self, offset: u64) -> bool {
        self.chunks
            .range(..=offset)
            .next_back()
            .map(|(&start, buf)| offset < start + buf.len() as u64)
            .unwrap_or(false)
    }

    fn evict_if_needed(&mut self, protect_offset: u64) {
        while self.total_bytes > self.max_bytes && self.chunks.len() > 1 {
            // Prefer evicting oldest chunks well behind the playback cursor.
            let candidate = self
                .chunks
                .iter()
                .find(|(&start, buf)| start + (buf.len() as u64) <= protect_offset)
                .or_else(|| self.chunks.iter().next_back())
                .map(|(&k, _)| k);
            let Some(key) = candidate else {
                break;
            };
            if let Some(removed) = self.chunks.remove(&key) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.len());
            }
        }
    }
}

