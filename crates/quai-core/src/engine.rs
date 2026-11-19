//! The refresh engine: forge → selected assets → RPM headers → signed repodata.
//!
//! Everything it learns is cached under `/var/cache/quai`:
//!
//! * `forge/<id>.json`: last release listing and its ETag,
//! * `headers/<key>.{bin,json}`: RPM headers, immutable per asset,
//! * `repos/<id>/repodata/`: generated metadata, content-addressed,
//! * `repos/<id>/state.json`: the [`Snapshot`] describing what is served.

use crate::config::{GlobalConfig, SourceConfig, Tokens};
use crate::error::{not_found, usage};
use crate::forge::{self, is_binary_rpm, Asset, Fetch, ForgeRef, RateLimit, Release};
use crate::fsutil::write_atomic;
use crate::paths::Paths;
use crate::repodata::{self, RepoPackage};
use crate::rpm::{self, Layout, PackageInfo};
use crate::sign::Signer;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use quai_types::{Channel, ChecksumSource, Excluded, Severity, VariantGroup, Warning};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;

/// What quaid serves for one source, persisted as `state.json`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Snapshot {
    pub fingerprint: String,
    pub config_hash: String,
    pub revision: i64,
    pub generated_at: Option<DateTime<Utc>>,
    pub last_check: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub latest_release: Option<String>,
    pub packages: Vec<ServedPackage>,
    pub excluded: Vec<Excluded>,
    pub signed: bool,
    pub rate_limit: Option<RateLimit>,
    pub files: Vec<String>,
    pub previous_files: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ServedPackage {
    pub name: String,
    pub evr: String,
    pub arch: String,
    pub release_tag: String,
    pub asset_key: String,
    pub file_name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub checksum: ChecksumSource,
}

#[derive(Serialize, Deserialize, Default)]
struct ForgeCache {
    etag: Option<String>,
    fetched_at: Option<DateTime<Utc>>,
    releases: Vec<Release>,
    rate_limit: Option<RateLimit>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HeaderMeta {
    pub url: String,
    pub layout: Layout,
    pub total_size: Option<u64>,
    /// SHA-256 computed by downloading the asset (forges without digests).
    pub sha256: Option<String>,
}

/// Which assets of which releases a source follows.
pub struct Selection {
    pub channel: Channel,
    pub pattern: Option<Regex>,
    pub keep: u32,
}

pub struct Candidate<'a> {
    pub release: &'a Release,
    pub assets: Vec<&'a Asset>,
}

/// Newest `keep` releases that match the channel and carry at least one binary RPM.
pub fn select<'a>(releases: &'a [Release], sel: &Selection) -> Vec<Candidate<'a>> {
    releases
        .iter()
        .filter(|r| !r.draft)
        .filter(|r| sel.channel == Channel::Prerelease || !r.prerelease)
        .filter_map(|r| {
            let assets: Vec<&Asset> = r
                .assets
                .iter()
                .filter(|a| is_binary_rpm(&a.name))
                .filter(|a| sel.pattern.as_ref().map(|p| p.is_match(&a.name)).unwrap_or(true))
                .collect();
            (!assets.is_empty()).then_some(Candidate { release: r, assets })
        })
        .take(sel.keep.max(1) as usize)
        .collect()
}

/// Stable cache key for an asset: changes whenever the file could have changed.
pub fn asset_key(a: &Asset) -> String {
    let material = format!(
        "{}|{}|{}|{}",
        a.url,
        a.size.map(|s| s.to_string()).unwrap_or_default(),
        a.sha256.clone().unwrap_or_default(),
        a.updated_at.map(|t| t.timestamp().to_string()).unwrap_or_default()
    );
    hex::encode(&Sha256::digest(material.as_bytes())[..12])
}

/// One package of the newest release, as found by [`Engine::preview`].
pub struct PreviewPackage {
    pub info: PackageInfo,
    pub asset: Asset,
    pub checksum: ChecksumSource,
    pub size: Option<u64>,
}

pub struct PreviewCore {
    pub forge_ref: ForgeRef,
    pub release: Release,
    /// Packages for every architecture in the newest matching release.
    pub packages: Vec<PreviewPackage>,
    pub variants: Vec<VariantGroup>,
    pub warnings: Vec<Warning>,
}

pub struct Engine {
    pub paths: Paths,
    pub global: GlobalConfig,
    tokens: Tokens,
    signer: Option<Signer>,
    http: reqwest::Client,
    /// quaid owns the cache; the CLI only reads it.
    cache_writable: bool,
}

fn warn(code: &str, severity: Severity, message: impl Into<String>) -> Warning {
    Warning { code: code.to_string(), severity, message: message.into() }
}

impl Engine {
    pub fn new(
        paths: Paths,
        global: GlobalConfig,
        tokens: Tokens,
        signer: Option<Signer>,
        cache_writable: bool,
    ) -> Result<Engine> {
        Ok(Engine { paths, global, tokens, signer, http: crate::http::client()?, cache_writable })
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Tokens are only ever sent to the API derived from the host they were saved for.
    fn token_for(&self, fref: &ForgeRef) -> Option<&str> {
        if fref.api_url.is_some() {
            return None;
        }
        self.tokens.get(&fref.host)
    }

    fn write_cache(&self, path: &std::path::Path, data: &[u8]) {
        if !self.cache_writable {
            return;
        }
        if let Err(e) = write_atomic(path, data, 0o644) {
            tracing::warn!("cannot write cache file {}: {e:#}", path.display());
        }
    }

    /// Release listing, using the on-disk cache and its ETag when `cache_id` is given.
    pub async fn fetch_releases(
        &self,
        fref: &ForgeRef,
        cache_id: Option<&str>,
    ) -> Result<(Vec<Release>, Option<RateLimit>)> {
        let cache_path = cache_id.map(|id| self.paths.forge_cache(id));
        let cached: Option<ForgeCache> =
            cache_path.as_ref().and_then(|p| fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok());
        let etag = cached.as_ref().and_then(|c| c.etag.as_deref());
        let token = self.token_for(fref);
        match forge::list_releases(&self.http, fref, token, etag).await? {
            Fetch::NotModified { rate_limit } => {
                let c = cached.unwrap_or_default();
                Ok((c.releases, rate_limit.or(c.rate_limit)))
            }
            Fetch::Fresh { releases, etag, rate_limit } => {
                if let Some(p) = cache_path {
                    let c = ForgeCache {
                        etag,
                        fetched_at: Some(Utc::now()),
                        releases: releases.clone(),
                        rate_limit: rate_limit.clone(),
                    };
                    if let Ok(bytes) = serde_json::to_vec(&c) {
                        self.write_cache(&p, &bytes);
                    }
                }
                Ok((releases, rate_limit))
            }
        }
    }

    fn cached_releases(&self, id: &str) -> Option<Vec<Release>> {
        let bytes = fs::read(self.paths.forge_cache(id)).ok()?;
        serde_json::from_slice::<ForgeCache>(&bytes).ok().map(|c| c.releases)
    }

    fn header_paths(&self, key: &str) -> (PathBuf, PathBuf) {
        let dir = self.paths.headers_dir();
        (dir.join(format!("{key}.bin")), dir.join(format!("{key}.json")))
    }

    /// Header of an asset, from cache or over HTTP.
    pub async fn header(&self, asset: &Asset) -> Result<(PackageInfo, HeaderMeta)> {
        let key = asset_key(asset);
        let (bin, meta_path) = self.header_paths(&key);
        if let (Ok(bytes), Ok(meta)) = (fs::read(&bin), fs::read(&meta_path)) {
            if let Ok(meta) = serde_json::from_slice::<HeaderMeta>(&meta) {
                if let Ok(info) = rpm::parse_package(&bytes, &meta.layout) {
                    return Ok((info, meta));
                }
            }
        }
        let fetched = rpm::fetch_header(&self.http, &asset.url)
            .await
            .with_context(|| format!("reading the RPM header of {}", asset.name))?;
        let info = rpm::parse_package(&fetched.bytes, &fetched.layout)
            .with_context(|| format!("parsing the RPM header of {}", asset.name))?;
        let meta =
            HeaderMeta { url: asset.url.clone(), layout: fetched.layout, total_size: fetched.total_size, sha256: None };
        self.write_cache(&bin, &fetched.bytes);
        if let Ok(m) = serde_json::to_vec(&meta) {
            self.write_cache(&meta_path, &m);
        }
        Ok((info, meta))
    }

    /// SHA-256 of an asset: from the forge, from cache, or by downloading it once.
    async fn checksum(&self, asset: &Asset, meta: &mut HeaderMeta) -> Result<(String, ChecksumSource, Option<u64>)> {
        if let Some(s) = &asset.sha256 {
            return Ok((s.clone(), ChecksumSource::Forge, None));
        }
        if let Some(s) = &meta.sha256 {
            return Ok((s.clone(), ChecksumSource::Computed, None));
        }
        tracing::info!("computing SHA-256 of {} (the forge publishes no digest)", asset.name);
        let (sum, size) = rpm::sha256_of_url(&self.http, &asset.url).await?;
        meta.sha256 = Some(sum.clone());
        meta.total_size = meta.total_size.or(Some(size));
        let (_, meta_path) = self.header_paths(&asset_key(asset));
        if let Ok(m) = serde_json::to_vec(meta) {
            self.write_cache(&meta_path, &m);
        }
        Ok((sum, ChecksumSource::Computed, Some(size)))
    }

    /// What adding `fref` would serve, without touching any configuration.
    pub async fn preview(&self, fref: &ForgeRef, channel: Channel, pattern: Option<&str>) -> Result<PreviewCore> {
        let pattern_re = crate::config::compile_pattern(pattern)?;
        let (releases, _) = self.fetch_releases(fref, None).await?;
        let sel = Selection { channel, pattern: pattern_re, keep: 1 };
        let cands = select(&releases, &sel);
        let Some(latest) = cands.first() else {
            return Err(explain_no_rpm(fref, &releases, channel, pattern.is_some()));
        };

        let mut packages = Vec::new();
        let mut warnings = Vec::new();
        for asset in &latest.assets {
            let (info, meta) = match self.header(asset).await {
                Ok(v) => v,
                Err(e) => {
                    warnings.push(warn("unreadable_asset", Severity::Warning, format!("{}: {e:#}", asset.name)));
                    continue;
                }
            };
            if info.is_source {
                continue;
            }
            let checksum = if asset.sha256.is_some() { ChecksumSource::Forge } else { ChecksumSource::Pending };
            let size = asset.size.or(meta.total_size);
            packages.push(PreviewPackage { info, asset: (*asset).clone(), checksum, size });
        }
        if packages.is_empty() {
            return Err(not_found(format!(
                "no readable binary RPM in {} release {}",
                fref.display(),
                latest.release.tag
            )));
        }

        // Same package name and architecture from several files: the user must choose.
        let mut groups: BTreeMap<(String, String), Vec<(String, String)>> = BTreeMap::new();
        for p in &packages {
            groups
                .entry((p.info.name.clone(), p.info.arch.clone()))
                .or_default()
                .push((p.asset.name.clone(), suggest_pattern(&p.asset.name, &p.info.version)));
        }
        let variants: Vec<VariantGroup> = groups
            .into_iter()
            .filter(|(_, assets)| assets.len() > 1)
            .map(|((name, arch), assets)| VariantGroup {
                name,
                arch,
                patterns: assets.iter().map(|(_, p)| p.clone()).collect(),
                assets: assets.into_iter().map(|(a, _)| a).collect(),
            })
            .collect();

        let names: HashSet<&str> = packages.iter().map(|p| p.info.name.as_str()).collect();
        for p in &packages {
            for o in &p.info.obsoletes {
                if !names.contains(o.name.as_str()) {
                    warnings.push(warn(
                        "obsoletes_foreign",
                        Severity::Danger,
                        format!(
                            "{} declares Obsoletes: {}; quai will not serve it unless you allow it explicitly",
                            p.info.name, o.name
                        ),
                    ));
                }
            }
        }
        if packages.iter().any(|p| !p.info.signed) {
            warnings.push(warn(
                "unsigned_packages",
                Severity::Info,
                "upstream does not sign these RPMs; integrity rests on the SHA-256 in quai's signed metadata",
            ));
        }
        if packages.iter().any(|p| p.checksum == ChecksumSource::Pending) {
            warnings.push(warn(
                "no_forge_digest",
                Severity::Info,
                format!("{} publishes no checksum: quaid downloads each version once to compute it", fref.host),
            ));
        }
        if latest.release.prerelease {
            warnings.push(warn("prerelease", Severity::Info, format!("{} is a pre-release", latest.release.tag)));
        }
        Ok(PreviewCore { forge_ref: fref.clone(), release: latest.release.clone(), packages, variants, warnings })
    }

    pub fn load_snapshot(paths: &Paths, id: &str) -> Option<Snapshot> {
        let bytes = fs::read(paths.state_file(id)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn save_snapshot(&self, id: &str, snap: &Snapshot) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(snap)?;
        write_atomic(&self.paths.state_file(id), &bytes, 0o644)
    }

    /// Path of a generated metadata file, refusing anything but plain file names.
    pub fn repodata_path(&self, id: &str, name: &str) -> Result<PathBuf> {
        crate::config::validate_id(id)?;
        let ok = !name.is_empty()
            && name.len() < 200
            && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            && !name.starts_with('.');
        if !ok {
            return Err(usage("invalid file name"));
        }
        Ok(self.paths.repodata_dir(id).join(name))
    }

    fn config_hash(&self, src: &SourceConfig) -> String {
        let material = serde_json::json!({
            "channel": src.channel,
            "pattern": src.asset_pattern,
            "packages": src.packages,
            "keep": src.keep_versions,
            "allow_obsoletes": src.allow_obsoletes,
            "project": src.project,
            "host": src.host,
            "sign": self.global.defaults.sign_metadata && self.signer.is_some(),
        });
        hex::encode(&Sha256::digest(material.to_string().as_bytes())[..16])
    }

    /// Bring a source's metadata up to date. `force` skips the check interval.
    ///
    /// When the forge cannot be reached the previous metadata keeps being served
    /// and the error is recorded in the snapshot.
    pub async fn refresh(&self, src: &SourceConfig, force: bool) -> Result<Snapshot> {
        let id = src.id.as_str();
        let mut snap = Self::load_snapshot(&self.paths, id).unwrap_or_default();
        let now = Utc::now();
        let interval = Duration::minutes(self.global.defaults.check_interval_mins as i64);
        let config_hash = self.config_hash(src);
        let have_repodata = self.paths.repodata_dir(id).join("repomd.xml").exists();
        let recently_checked = snap.last_check.map(|t| now - t < interval).unwrap_or(false);

        if !force && recently_checked && have_repodata && snap.config_hash == config_hash {
            return Ok(snap);
        }

        let fref = src.forge_ref();
        let use_cache_only = !force && recently_checked;
        let releases = match (use_cache_only, self.cached_releases(id)) {
            (true, Some(r)) => r,
            _ => {
                snap.last_check = Some(now);
                match self.fetch_releases(&fref, Some(id)).await {
                    Ok((r, rate)) => {
                        snap.rate_limit = rate;
                        r
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        tracing::warn!("{id}: {msg}");
                        snap.last_error = Some(msg);
                        self.save_snapshot(id, &snap).ok();
                        if have_repodata {
                            return Ok(snap);
                        }
                        return Err(e);
                    }
                }
            }
        };

        let sel = Selection { channel: src.channel, pattern: src.asset_regex()?, keep: src.keep_versions };
        let cands = select(&releases, &sel);
        let allowed: HashSet<&str> = src.packages.iter().map(|s| s.as_str()).collect();
        let allowed_obsoletes: HashSet<&str> = src.allow_obsoletes.iter().map(|s| s.as_str()).collect();

        let mut repo_pkgs: Vec<RepoPackage> = Vec::new();
        let mut served: Vec<ServedPackage> = Vec::new();
        let mut excluded: Vec<Excluded> = Vec::new();
        let mut seen_nevra = HashSet::new();
        let mut errors = Vec::new();

        for cand in &cands {
            for asset in &cand.assets {
                let (info, mut meta) = match self.header(asset).await {
                    Ok(v) => v,
                    Err(e) => {
                        errors.push(format!("{}: {e:#}", asset.name));
                        continue;
                    }
                };
                if info.is_source {
                    continue;
                }
                if !allowed.contains(info.name.as_str()) {
                    excluded.push(Excluded {
                        name: info.name.clone(),
                        file: asset.name.clone(),
                        reason: format!(
                            "not in this source's package list (allow it with `quai set {id} allow-package {}`)",
                            info.name
                        ),
                    });
                    continue;
                }
                if let Some(o) = info
                    .obsoletes
                    .iter()
                    .find(|o| !allowed.contains(o.name.as_str()) && !allowed_obsoletes.contains(o.name.as_str()))
                {
                    excluded.push(Excluded {
                        name: info.name.clone(),
                        file: asset.name.clone(),
                        reason: format!(
                            "obsoletes '{}', which this source does not own (allow it with `quai set {id} allow-obsoletes {}`)",
                            o.name, o.name
                        ),
                    });
                    continue;
                }
                if !seen_nevra.insert(info.nevra()) {
                    continue;
                }
                let (sha256, checksum, dl_size) = match self.checksum(asset, &mut meta).await {
                    Ok(v) => v,
                    Err(e) => {
                        errors.push(format!("{}: checksum unavailable: {e:#}", asset.name));
                        continue;
                    }
                };
                let size = asset.size.or(meta.total_size).or(dl_size).unwrap_or(0);
                let key = asset_key(asset);
                let file_time = asset
                    .updated_at
                    .or(cand.release.published_at)
                    .map(|t| t.timestamp())
                    .unwrap_or(info.build_time as i64);
                served.push(ServedPackage {
                    name: info.name.clone(),
                    evr: info.evr(),
                    arch: info.arch.clone(),
                    release_tag: cand.release.tag.clone(),
                    asset_key: key.clone(),
                    file_name: asset.name.clone(),
                    url: asset.url.clone(),
                    size,
                    sha256: sha256.clone(),
                    checksum,
                });
                repo_pkgs.push(RepoPackage {
                    location: format!("packages/{key}/{}", asset.name),
                    header_start: meta.layout.header_start,
                    header_end: meta.layout.header_end,
                    package_size: size,
                    file_time,
                    sha256,
                    info,
                });
            }
        }

        repo_pkgs.sort_by(|a, b| {
            a.info
                .name
                .cmp(&b.info.name)
                .then_with(|| crate::evr::compare_evr(&b.info.evr(), &a.info.evr()))
                .then_with(|| a.info.arch.cmp(&b.info.arch))
        });
        served.sort_by(|a, b| {
            a.name.cmp(&b.name).then_with(|| crate::evr::compare_evr(&b.evr, &a.evr)).then_with(|| a.arch.cmp(&b.arch))
        });

        let mut fp = Sha256::new();
        fp.update(config_hash.as_bytes());
        for p in &served {
            fp.update(format!("{}|{}\n", p.asset_key, p.sha256).as_bytes());
        }
        let fingerprint = hex::encode(&fp.finalize()[..16]);

        snap.latest_release = cands.first().map(|c| c.release.tag.clone());
        snap.excluded = excluded;
        snap.last_error = if errors.is_empty() { None } else { Some(errors.join("; ")) };
        if cands.is_empty() {
            snap.last_error =
                Some(format!("{}", explain_no_rpm(&fref, &releases, src.channel, src.asset_pattern.is_some())));
        }

        let sign = self.global.defaults.sign_metadata;
        if sign && self.signer.is_none() {
            snap.last_error = Some("signing key missing: run `quai setup`".to_string());
        }

        if fingerprint != snap.fingerprint || !have_repodata {
            let newest = cands.iter().filter_map(|c| c.release.published_at).map(|t| t.timestamp()).max();
            let mut revision = newest.unwrap_or_else(|| now.timestamp());
            if revision <= snap.revision {
                revision = snap.revision + 1;
            }
            self.write_repodata(id, &repo_pkgs, revision, &mut snap)?;
            snap.fingerprint = fingerprint;
            snap.revision = revision;
            snap.generated_at = Some(now);
            tracing::info!("{id}: serving {} package(s), revision {revision}", served.len());
        }
        snap.config_hash = config_hash;
        snap.packages = served;
        if snap.last_error.is_none() {
            snap.last_success = Some(now);
        }
        self.save_snapshot(id, &snap)?;
        Ok(snap)
    }

    fn write_repodata(&self, id: &str, pkgs: &[RepoPackage], revision: i64, snap: &mut Snapshot) -> Result<()> {
        let dir = self.paths.repodata_dir(id);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let data = repodata::build(pkgs, revision);
        let mut names = Vec::new();
        for f in &data.files {
            write_atomic(&dir.join(&f.name), &f.bytes, 0o644)?;
            names.push(f.name.clone());
        }
        let asc = dir.join("repomd.xml.asc");
        let signed = match (&self.signer, self.global.defaults.sign_metadata) {
            (Some(signer), true) => {
                let sig = signer.sign_detached(&data.repomd)?;
                write_atomic(&asc, sig.as_bytes(), 0o644)?;
                true
            }
            _ => {
                let _ = fs::remove_file(&asc);
                false
            }
        };
        write_atomic(&dir.join("repomd.xml"), &data.repomd, 0o644)?;

        // Keep this generation and the previous one (a dnf may be mid-download).
        let previous = std::mem::replace(&mut snap.files, names.clone());
        snap.previous_files = previous;
        let keep: HashSet<String> = snap
            .files
            .iter()
            .chain(snap.previous_files.iter())
            .cloned()
            .chain(["repomd.xml".to_string(), "repomd.xml.asc".to_string()])
            .collect();
        if let Ok(entries) = fs::read_dir(&dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !keep.contains(&name) && !name.starts_with('.') {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
        snap.signed = signed;
        Ok(())
    }
}

/// Regex matching an asset name whatever its version:
/// `app-1.2.3-el9.x86_64.rpm` → `^app-.*-el9\.x86_64\.rpm$`.
pub fn suggest_pattern(asset: &str, version: &str) -> String {
    let escaped = regex::escape(asset);
    let v = regex::escape(version);
    let body = if !version.is_empty() && escaped.contains(&v) { escaped.replacen(&v, ".*", 1) } else { escaped };
    format!("^{body}$")
}

/// Explain why no release qualified, in the user's terms.
fn explain_no_rpm(fref: &ForgeRef, releases: &[Release], channel: Channel, has_pattern: bool) -> anyhow::Error {
    if releases.is_empty() {
        return not_found(format!("{} has no releases", fref.display()));
    }
    let with_rpm: Vec<&Release> = releases.iter().filter(|r| r.assets.iter().any(|a| is_binary_rpm(&a.name))).collect();
    if with_rpm.is_empty() {
        let mut kinds: Vec<String> = releases
            .iter()
            .take(5)
            .flat_map(|r| r.assets.iter())
            .filter_map(|a| a.name.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()))
            .collect();
        kinds.sort();
        kinds.dedup();
        let found = if kinds.is_empty() { "no assets".to_string() } else { format!("only .{}", kinds.join(", .")) };
        return not_found(format!(
            "{} publishes no .rpm in its last {} releases ({found}); quai only manages RPM packages",
            fref.display(),
            releases.len()
        ));
    }
    if channel == Channel::Stable && with_rpm.iter().all(|r| r.prerelease) {
        return not_found(format!(
            "{} only ships RPMs in pre-releases; use the prerelease channel to follow them",
            fref.display()
        ));
    }
    if has_pattern {
        return not_found(format!("no RPM asset of {} matches the asset pattern", fref.display()));
    }
    not_found(format!("no usable RPM release found for {}", fref.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_suggestion_drops_the_version() {
        let p = suggest_pattern("app-1.2.3-el9.x86_64.rpm", "1.2.3");
        let re = regex::Regex::new(&p).unwrap();
        assert!(re.is_match("app-1.2.3-el9.x86_64.rpm"));
        assert!(re.is_match("app-1.3.0-el9.x86_64.rpm"));
        assert!(!re.is_match("app-1.3.0-fc40.x86_64.rpm"));
    }

    fn release(tag: &str, pre: bool, assets: &[&str]) -> Release {
        Release {
            tag: tag.into(),
            name: None,
            prerelease: pre,
            draft: false,
            published_at: None,
            assets: assets
                .iter()
                .map(|a| Asset {
                    id: a.to_string(),
                    name: a.to_string(),
                    size: None,
                    url: format!("http://x/{a}"),
                    sha256: None,
                    updated_at: None,
                })
                .collect(),
        }
    }

    #[test]
    fn selection_respects_channel_pattern_and_keep() {
        let rels = vec![
            release("v3-rc", true, &["a-3.x86_64.rpm"]),
            release("v2", false, &["a-2.x86_64.rpm", "a-2.src.rpm", "a-2.AppImage"]),
            release("v1.5", false, &["a-1.5.AppImage"]),
            release("v1", false, &["a-1.x86_64.rpm"]),
        ];
        let stable = Selection { channel: Channel::Stable, pattern: None, keep: 3 };
        let tags: Vec<&str> = select(&rels, &stable).iter().map(|c| c.release.tag.as_str()).collect();
        assert_eq!(tags, ["v2", "v1"]);
        assert_eq!(select(&rels, &stable)[0].assets.len(), 1, "src.rpm and AppImage are ignored");
        let pre = Selection { channel: Channel::Prerelease, pattern: None, keep: 1 };
        assert_eq!(select(&rels, &pre)[0].release.tag, "v3-rc");
        let pat = Selection { channel: Channel::Stable, pattern: Some(Regex::new("^a-1").unwrap()), keep: 3 };
        assert_eq!(select(&rels, &pat).len(), 1);
    }
}
