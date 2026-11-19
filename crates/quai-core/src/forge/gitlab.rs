//! GitLab releases. Assets are "links"; GitLab reports neither size nor digest,
//! so quaid learns the size from the range request and computes the SHA-256.

use super::{etag_of, rate_limit_from, status_error, Asset, Fetch, ForgeRef, Release, PAGE_SIZE};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;

#[derive(Deserialize)]
struct GlRelease {
    tag_name: String,
    name: Option<String>,
    released_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    upcoming_release: bool,
    #[serde(default)]
    assets: GlAssets,
}

#[derive(Deserialize, Default)]
struct GlAssets {
    #[serde(default)]
    links: Vec<GlLink>,
}

#[derive(Deserialize)]
struct GlLink {
    id: u64,
    name: String,
    url: String,
    direct_asset_url: Option<String>,
}

pub(super) async fn list(
    http: &reqwest::Client,
    fref: &ForgeRef,
    token: Option<&str>,
    etag: Option<&str>,
) -> Result<Fetch> {
    let encoded = utf8_percent_encode(&fref.project, NON_ALPHANUMERIC).to_string();
    let url = format!("{}/projects/{encoded}/releases?per_page={PAGE_SIZE}", fref.api_base());
    let mut req = http.get(&url).timeout(crate::http::API_TIMEOUT).header("Accept", "application/json");
    if let Some(t) = token {
        req = req.header("PRIVATE-TOKEN", t);
    }
    if let Some(e) = etag {
        req = req.header(reqwest::header::IF_NONE_MATCH, e);
    }
    let resp = req.send().await.with_context(|| format!("contacting {}", fref.host))?;
    let rate_limit = rate_limit_from(resp.headers());
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Fetch::NotModified { rate_limit });
    }
    if !resp.status().is_success() {
        return Err(status_error(fref, &resp));
    }
    let new_etag = etag_of(resp.headers());
    let raw: Vec<GlRelease> = resp.json().await.context("decoding GitLab releases")?;
    let releases = raw
        .into_iter()
        .map(|r| Release {
            tag: r.tag_name,
            name: r.name.filter(|n| !n.is_empty()),
            // GitLab has no pre-release flag; an upcoming release is the closest thing.
            prerelease: r.upcoming_release,
            draft: false,
            published_at: r.released_at.or(r.created_at),
            assets: r
                .assets
                .links
                .into_iter()
                .map(|l| Asset {
                    id: l.id.to_string(),
                    name: l.name,
                    size: None,
                    url: l.direct_asset_url.unwrap_or(l.url),
                    sha256: None,
                    updated_at: None,
                })
                .collect(),
        })
        .collect();
    Ok(Fetch::Fresh { releases, etag: new_etag, rate_limit })
}
