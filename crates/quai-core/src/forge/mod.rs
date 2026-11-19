//! Release listings from GitHub, GitLab and Forgejo/Gitea, normalised.

mod forgejo;
mod github;
mod gitlab;

use crate::error::{network, not_found, QuaiError};
use crate::Kind;
use anyhow::Result;
use chrono::{DateTime, Utc};
use quai_types::ForgeKind;
use serde::{Deserialize, Serialize};

/// A project on a forge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeRef {
    pub forge: ForgeKind,
    pub host: String,
    pub project: String,
    /// API base override (GitHub Enterprise, tests). Derived from `host` when absent.
    pub api_url: Option<String>,
}

impl ForgeRef {
    pub fn api_base(&self) -> String {
        if let Some(u) = &self.api_url {
            return u.trim_end_matches('/').to_string();
        }
        match self.forge {
            ForgeKind::Github if self.host == "github.com" => "https://api.github.com".to_string(),
            ForgeKind::Github => format!("https://{}/api/v3", self.host),
            ForgeKind::Gitlab => format!("https://{}/api/v4", self.host),
            ForgeKind::Forgejo => format!("https://{}/api/v1", self.host),
        }
    }

    pub fn web_url(&self) -> String {
        format!("https://{}/{}", self.host, self.project)
    }

    pub fn display(&self) -> String {
        format!("{}/{}", self.host, self.project)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// Forge-specific id, stable for the life of the asset.
    pub id: String,
    pub name: String,
    pub size: Option<u64>,
    /// Public download URL.
    pub url: String,
    /// Lowercase hex SHA-256 published by the forge, if any.
    pub sha256: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    pub name: Option<String>,
    pub prerelease: bool,
    pub draft: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RateLimit {
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    /// Unix time at which the budget resets.
    pub reset: Option<i64>,
}

pub enum Fetch {
    NotModified { rate_limit: Option<RateLimit> },
    Fresh { releases: Vec<Release>, etag: Option<String>, rate_limit: Option<RateLimit> },
}

/// How many releases to ask for. Enough to find `keep_versions` with RPMs among
/// releases that only ship other formats or are pre-releases.
pub const PAGE_SIZE: u32 = 30;

pub async fn list_releases(
    http: &reqwest::Client,
    fref: &ForgeRef,
    token: Option<&str>,
    etag: Option<&str>,
) -> Result<Fetch> {
    let mut fetch = match fref.forge {
        ForgeKind::Github => github::list(http, fref, token, etag).await?,
        ForgeKind::Gitlab => gitlab::list(http, fref, token, etag).await?,
        ForgeKind::Forgejo => forgejo::list(http, fref, token, etag).await?,
    };
    if let Fetch::Fresh { releases, .. } = &mut fetch {
        sort_newest_first(releases);
    }
    Ok(fetch)
}

pub fn sort_newest_first(releases: &mut [Release]) {
    // Stable sort keeps the forge's own order for releases without a date.
    releases.sort_by(|a, b| b.published_at.cmp(&a.published_at));
}

/// Turn an HTTP status into a helpful error.
pub(crate) fn status_error(fref: &ForgeRef, resp: &reqwest::Response) -> anyhow::Error {
    let status = resp.status();
    let remaining = resp
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    match status.as_u16() {
        404 => not_found(format!(
            "{} not found (private project, typo, or no access with the configured token)",
            fref.display()
        )),
        401 => QuaiError::new(Kind::Permission, format!("{} rejected the token (401)", fref.host)),
        403 | 429 if remaining == Some(0) => network(format!(
            "{} API rate limit reached; add a token with `quai token set --host {}`",
            fref.host, fref.host
        )),
        _ => network(format!("{} answered HTTP {} for {}", fref.host, status, fref.display())),
    }
}

pub(crate) fn rate_limit_from(headers: &reqwest::header::HeaderMap) -> Option<RateLimit> {
    let num = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<i64>().ok());
    let limit = num("x-ratelimit-limit").or_else(|| num("ratelimit-limit"));
    let remaining = num("x-ratelimit-remaining").or_else(|| num("ratelimit-remaining"));
    let reset = num("x-ratelimit-reset").or_else(|| num("ratelimit-reset"));
    if limit.is_none() && remaining.is_none() {
        return None;
    }
    Some(RateLimit { limit: limit.map(|v| v as u64), remaining: remaining.map(|v| v as u64), reset })
}

pub(crate) fn etag_of(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers.get(reqwest::header::ETAG).and_then(|v| v.to_str().ok()).map(|s| s.to_string())
}

/// Is this asset an installable binary RPM?
pub fn is_binary_rpm(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".rpm") && !n.ends_with(".src.rpm") && !n.ends_with(".nosrc.rpm")
}
