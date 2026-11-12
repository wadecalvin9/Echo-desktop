use echo_youtube::ChunkBuffer;
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE, USER_AGENT};
use std::io::{self, Read, Seek, SeekFrom};
use std::time::Duration;
use symphonia::core::io::MediaSource;

const CHUNK_SIZE: u64 = 256 * 1024;
const MAX_CACHE_BYTES: usize = 24 * 1024 * 1024;

pub struct HttpRangeSource {
    client: Client,
    url: String,
    user_agent: String,
    pos: u64,
    content_length: Option<u64>,
    buffer: ChunkBuffer,
}

impl HttpRangeSource {
    pub fn open(url: String, user_agent: String, known_len: Option<u64>) -> io::Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let mut source = Self {
            client,
            url,
            user_agent,
            pos: 0,
            content_length: known_len,
            buffer: ChunkBuffer::new(known_len, MAX_CACHE_BYTES),
        };

        // Prefetch the first chunk immediately so probing succeeds without extra round-trips.
        source.fetch_chunk_at(0)?;
        Ok(source)
    }

    fn fetch_chunk_at(&mut self, offset: u64) -> io::Result<()> {
        if let Some(len) = self.content_length {
            if offset >= len {
                return Ok(());
            }
        }
        let end = match self.content_length {
            Some(len) => (offset + CHUNK_SIZE - 1).min(len.saturating_sub(1)),
            None => offset + CHUNK_SIZE - 1,
        };
        let range_hdr = format!("bytes={offset}-{end}");
        tracing::debug!("HttpRangeSource fetch {range_hdr}");

        let mut last_err = None;
        for attempt in 0..2 {
            let resp = self
                .client
                .get(&self.url)
                .header(USER_AGENT, &self.user_agent)
                .header(RANGE, &range_hdr)
                .send();

            match resp {
                Ok(r) if r.status().is_success() => {
                    if self.content_length.is_none() {
                        if let Some(total) = r
                            .headers()
                            .get(CONTENT_RANGE)
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.split('/').nth(1))
                            .and_then(|n| n.parse::<u64>().ok())
                        {
                            self.content_length = Some(total);
                            self.buffer.content_length = Some(total);
                        } else if let Some(cl) = r
                            .headers()
                            .get(CONTENT_LENGTH)
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.parse::<u64>().ok())
                        {
                            if offset == 0 && r.status() == reqwest::StatusCode::OK {
                                self.content_length = Some(cl);
                                self.buffer.content_length = Some(cl);
                            }
                        }
                    }

                    let bytes = r
                        .bytes()
                        .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))?;
                    self.buffer.insert(offset, bytes.to_vec(), self.pos);
                    return Ok(());
                }
                Ok(r) => {
                    let status = r.status();
                    last_err = Some(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!("HTTP {status}"),
                    ));
                    if status.as_u16() == 403 || status.as_u16() == 404 {
                        break;
                    }
                }
                Err(e) => {
                    last_err = Some(io::Error::new(io::ErrorKind::TimedOut, e));
                }
            }
            if attempt == 0 {
                std::thread::sleep(Duration::from_millis(120));
            }
        }

        Err(last_err.unwrap_or_else(|| io::Error::new(io::ErrorKind::Other, "range fetch failed")))
    }
}

impl Read for HttpRangeSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(len) = self.content_length {
            if self.pos >= len {
                return Ok(0);
            }
        }

        if !self.buffer.has_byte(self.pos) {
            self.fetch_chunk_at(self.pos)?;
        }

        let n = self.buffer.read_at(self.pos, buf);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for HttpRangeSource {
    fn seek(&mut self, style: SeekFrom) -> io::Result<u64> {
        let new_pos: i64 = match style {
            SeekFrom::Start(p) => p as i64,
            SeekFrom::Current(delta) => self.pos as i64 + delta,
            SeekFrom::End(delta) => {
                let len = self.content_length.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::Unsupported, "unknown stream length")
                })?;
                len as i64 + delta
            }
        };
        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "negative seek position",
            ));
        }
        self.pos = new_pos as u64;
        Ok(self.pos)
    }
}

impl MediaSource for HttpRangeSource {
    fn is_seekable(&self) -> bool {
        self.content_length.is_some()
    }

    fn byte_len(&self) -> Option<u64> {
        self.content_length
    }
}

