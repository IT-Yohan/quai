//! Forgejo and Gitea (Codeberg included) share the same releases API.

use super::{etag_of, rate_limit_from, status_error, Asset, Fetch, ForgeRef, Release, PAGE_SIZE};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use quai_i18n::{tr, trf};
use serde::Deserialize;

#[derive(Deserialize)]
struct FjRelease {
    tag_name: String,
    name: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    published_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    assets: Vec<FjAsset>,
}

#[derive(Deserialize)]
struct FjAsset {
    id: u64,
    name: String,
    size: Option<u64>,
    browser_download_url: String,
    created_at: Option<DateTime<Utc>>,
}

pub(super) async fn list(
    http: &reqwest::Client,
    fref: &ForgeRef,
    token: Option<&str>,
    etag: Option<&str>,
) -> Result<Fetch> {
    let url = format!("{}/repos/{}/releases?limit={PAGE_SIZE}", fref.api_base(), fref.project);
    let mut req = http.get(&url).timeout(crate::http::API_TIMEOUT).header("Accept", "application/json");
    if let Some(t) = token {
        req = req.header("Authorization", format!("token {t}"));
    }
    if let Some(e) = etag {
        req = req.header(reqwest::header::IF_NONE_MATCH, e);
    }
    let resp = req.send().await.with_context(|| trf("contacting {host}", &[("host", &fref.host)]))?;
    let rate_limit = rate_limit_from(resp.headers());
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(Fetch::NotModified { rate_limit });
    }
    if !resp.status().is_success() {
        return Err(status_error(fref, &resp));
    }
    let new_etag = etag_of(resp.headers());
    let raw: Vec<FjRelease> = resp.json().await.with_context(|| tr("decoding Forgejo releases"))?;
    let releases = raw
        .into_iter()
        .map(|r| Release {
            tag: r.tag_name,
            name: r.name.filter(|n| !n.is_empty()),
            prerelease: r.prerelease,
            draft: r.draft,
            published_at: r.published_at.or(r.created_at),
            assets: r
                .assets
                .into_iter()
                .map(|a| Asset {
                    id: a.id.to_string(),
                    name: a.name,
                    size: a.size,
                    url: a.browser_download_url,
                    sha256: None,
                    updated_at: a.created_at,
                })
                .collect(),
        })
        .collect();
    Ok(Fetch::Fresh { releases, etag: new_etag, rate_limit })
}
