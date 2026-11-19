//! Read an RPM header over HTTP without downloading the payload.

use super::header::{probe, Layout, Probe};
use crate::error::network;
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};

/// First request size. Small packages fit entirely; big ones need a second request.
const FIRST_CHUNK: usize = 64 * 1024;

pub struct FetchedHeader {
    /// Bytes `0..layout.header_end` of the file.
    pub bytes: Vec<u8>,
    pub layout: Layout,
    /// Size of the whole file, when the server said so.
    pub total_size: Option<u64>,
}

/// GET `url` restricted to `start..=end`. Works whether or not the server honours `Range`.
async fn get_range(http: &reqwest::Client, url: &str, start: usize, end: usize) -> Result<(Vec<u8>, Option<u64>)> {
    let resp = http
        .get(url)
        .timeout(crate::http::API_TIMEOUT)
        .header(RANGE, format!("bytes={start}-{end}"))
        .send()
        .await
        .with_context(|| format!("downloading header of {url}"))?;
    let status = resp.status();
    let want = end - start + 1;
    match status {
        StatusCode::PARTIAL_CONTENT => {
            let total = resp
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.rsplit('/').next())
                .and_then(|v| v.parse::<u64>().ok());
            let mut out = Vec::with_capacity(want);
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk?);
                if out.len() >= want {
                    out.truncate(want);
                    break;
                }
            }
            Ok((out, total))
        }
        StatusCode::OK => {
            // Range ignored: read from the start and stop once we have enough.
            let total =
                resp.headers().get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
            let mut out = Vec::with_capacity(end + 1);
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk?);
                if out.len() > end {
                    break;
                }
            }
            if out.len() <= start {
                return Ok((Vec::new(), total));
            }
            let stop = out.len().min(end + 1);
            Ok((out[start..stop].to_vec(), total))
        }
        StatusCode::RANGE_NOT_SATISFIABLE => Ok((Vec::new(), None)),
        _ => Err(network(format!("HTTP {status} while reading {url}"))),
    }
}

/// Read the lead, signature header and main header of the RPM at `url`.
pub async fn fetch_header(http: &reqwest::Client, url: &str) -> Result<FetchedHeader> {
    let (mut buf, mut total) = get_range(http, url, 0, FIRST_CHUNK - 1).await?;
    // Two rounds are enough for any sane file; four leaves room for odd layouts.
    for _ in 0..4 {
        let need = match probe(&buf)? {
            Probe::NeedBytes(n) => n,
            Probe::Layout(layout) => {
                if buf.len() >= layout.header_end {
                    buf.truncate(layout.header_end);
                    return Ok(FetchedHeader { bytes: buf, layout, total_size: total });
                }
                layout.header_end
            }
        };
        if need <= buf.len() {
            bail!("{url}: inconsistent RPM header");
        }
        let (more, t) = get_range(http, url, buf.len(), need - 1).await?;
        if more.is_empty() {
            bail!("{url} ended before the end of its RPM header");
        }
        total = total.or(t);
        buf.extend_from_slice(&more);
    }
    bail!("{url}: could not read the RPM header")
}

/// Download `url` once to compute its SHA-256 (forges that publish no digest).
pub async fn sha256_of_url(http: &reqwest::Client, url: &str) -> Result<(String, u64)> {
    let resp = http
        .get(url)
        .timeout(std::time::Duration::from_secs(3600))
        .send()
        .await
        .with_context(|| format!("downloading {url}"))?;
    if !resp.status().is_success() {
        return Err(network(format!("HTTP {} while downloading {url}", resp.status())));
    }
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        size += chunk.len() as u64;
        hasher.update(&chunk);
    }
    Ok((hex::encode(hasher.finalize()), size))
}
