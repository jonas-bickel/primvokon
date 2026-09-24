//! Video frames: the MJPEG multipart stream from `/streamer/stream` and a snapshot poller
//! fallback. Both yield JPEG-encoded frames as [`Bytes`].

use std::time::Duration;

use bytes::{Bytes, BytesMut};
use futures_util::{Stream, StreamExt};

use crate::client::PikvmClient;
use crate::error::{PikvmError, Result};
use crate::models::SnapshotOptions;

/// Incremental parser for `multipart/x-mixed-replace` bodies as produced by µStreamer.
#[derive(Debug, Default)]
pub struct MultipartParser {
    buf: BytesMut,
}

impl MultipartParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed bytes and return every complete JPEG part found so far.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Bytes> {
        self.buf.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some(header_end) = find(&self.buf, b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&self.buf[..header_end]).to_string();
            let content_length = header.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.trim()
                    .eq_ignore_ascii_case("content-length")
                    .then(|| v.trim().parse::<usize>().ok())?
            });
            let body_start = header_end + 4;
            match content_length {
                Some(len) => {
                    if self.buf.len() < body_start + len {
                        break;
                    }
                    let frame = self.buf.split_to(body_start + len).split_off(body_start).freeze();
                    frames.push(frame);
                    self.skip_trailing_newlines();
                }
                None => {
                    // No length: search for the next boundary marker after the header.
                    let Some(rel) = find(&self.buf[body_start..], b"\r\n--") else {
                        break;
                    };
                    let frame = self.buf.split_to(body_start + rel).split_off(body_start).freeze();
                    if !frame.is_empty() {
                        frames.push(frame);
                    }
                    self.skip_trailing_newlines();
                }
            }
        }
        // Guard against pathological growth when no header ever appears.
        if self.buf.len() > 64 * 1024 * 1024 {
            self.buf.clear();
        }
        frames
    }

    fn skip_trailing_newlines(&mut self) {
        while self.buf.starts_with(b"\r\n") {
            let _ = self.buf.split_to(2);
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Open the MJPEG stream; the returned stream yields JPEG frames until it is dropped or fails.
pub async fn open_mjpeg(client: &PikvmClient) -> Result<impl Stream<Item = Result<Bytes>>> {
    let resp = client.stream_request(client.stream_url()).await?.send().await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(crate::client::map_status(status, body));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if !content_type.contains("multipart") {
        return Err(PikvmError::Stream(format!("unexpected content type {content_type}")));
    }
    let body = resp.bytes_stream();
    let parser = MultipartParser::new();
    Ok(body
        .scan(parser, |parser, chunk| {
            let out: Vec<Result<Bytes>> = match chunk {
                Ok(bytes) => parser.push(&bytes).into_iter().map(Ok).collect(),
                Err(e) => vec![Err(PikvmError::Transport(e))],
            };
            futures_util::future::ready(Some(futures_util::stream::iter(out)))
        })
        .flatten())
}

/// Poll `/api/streamer/snapshot` every `interval`; used when MJPEG is unavailable.
pub fn snapshot_stream(client: PikvmClient, interval: Duration) -> impl Stream<Item = Result<Bytes>> {
    futures_util::stream::unfold((client, interval, true), |(client, interval, first)| async move {
        if !first {
            tokio::time::sleep(interval).await;
        }
        let opts = SnapshotOptions {
            allow_offline: true,
            ..Default::default()
        };
        let item = client.streamer().snapshot(&opts).await;
        Some((item, (client, interval, false)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frames_with_content_length_across_chunks() {
        let mut p = MultipartParser::new();
        let part =
            b"--boundarydonotcross\r\nX-Timestamp: 1\r\nContent-Type: image/jpeg\r\nContent-Length: 5\r\n\r\nHELLO\r\n";
        let mut data = Vec::new();
        data.extend_from_slice(part);
        data.extend_from_slice(part);
        let mid = 40;
        let f1 = p.push(&data[..mid]);
        assert!(f1.is_empty());
        let f2 = p.push(&data[mid..]);
        assert_eq!(f2.len(), 2);
        assert_eq!(&f2[0][..], b"HELLO");
    }

    #[test]
    fn parses_frames_without_content_length() {
        let mut p = MultipartParser::new();
        let data =
            b"--b\r\nContent-Type: image/jpeg\r\n\r\nJPEG1\r\n--b\r\nContent-Type: image/jpeg\r\n\r\nJPEG2\r\n--b\r\n";
        let frames = p.push(data);
        assert_eq!(frames.len(), 2);
        assert_eq!(&frames[1][..], b"JPEG2");
    }
}
