use super::{etag_of, rate_limit_from, status_error, Asset, Fetch, ForgeRef, Release, PAGE_SIZE};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use quai_i18n::{tr, trf};
use serde::Deserialize;

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    name: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    id: u64,
    name: String,
    size: Option<u64>,
    browser_download_url: String,
    updated_at: Option<DateTime<Utc>>,
    /// "sha256:<hex>", published for assets uploaded since June 2025.
    digest: Option<String>,
}

pub(super) async fn list(
    http: &reqwest::Client,
    fref: &ForgeRef,
    token: Option<&str>,
    etag: Option<&str>,
) -> Result<Fetch> {
    let url = format!("{}/repos/{}/releases?per_page={PAGE_SIZE}", fref.api_base(), fref.project);
    let mut req = http
        .get(&url)
        .timeout(crate::http::API_TIMEOUT)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(t) = token {
        req = req.bearer_auth(t);
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
    let raw: Vec<GhRelease> = resp.json().await.with_context(|| tr("decoding GitHub releases"))?;
    let releases = raw
        .into_iter()
        .map(|r| Release {
            tag: r.tag_name,
            name: r.name.filter(|n| !n.is_empty()),
            prerelease: r.prerelease,
            draft: r.draft,
            published_at: r.published_at,
            assets: r
                .assets
                .into_iter()
                .map(|a| Asset {
                    id: a.id.to_string(),
                    name: a.name,
                    size: a.size,
                    url: a.browser_download_url,
                    sha256: a
                        .digest
                        .and_then(|d| d.strip_prefix("sha256:").map(|h| h.to_ascii_lowercase()))
                        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit())),
                    updated_at: a.updated_at,
                })
                .collect(),
        })
        .collect();
    Ok(Fetch::Fresh { releases, etag: new_etag, rate_limit })
}
