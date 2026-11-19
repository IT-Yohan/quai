//! `/etc/quai/quai.toml`, `/etc/quai/sources.d/*.toml` and the token store.

use crate::error::{conflict, not_found, usage};
use crate::forge::ForgeRef;
use crate::fsutil::{chown_root_group, write_atomic};
use crate::paths::Paths;
use anyhow::{Context, Result};
use quai_i18n::{tr, trf};
use quai_types::{Channel, ForgeKind, SourceView};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::sync::OnceLock;

pub const DEFAULT_LISTEN: &str = "127.0.0.1:741";
pub const SYSTEM_GROUP: &str = "quai";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct GlobalConfig {
    pub daemon: DaemonConfig,
    pub defaults: Defaults,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct DaemonConfig {
    /// Address quaid listens on. Must match `quaid.socket` (`quai setup` keeps them aligned).
    pub listen: String,
    /// quaid exits after this many idle seconds; systemd restarts it on demand. 0 = never.
    pub idle_timeout_secs: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        DaemonConfig { listen: DEFAULT_LISTEN.to_string(), idle_timeout_secs: 120 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Defaults {
    pub channel: Channel,
    pub keep_versions: u32,
    /// Minimum minutes between two checks of the same forge.
    pub check_interval_mins: u32,
    /// Written as `metadata_expire` in generated `.repo` files.
    pub metadata_expire: String,
    /// Sign `repomd.xml` and set `repo_gpgcheck=1`.
    pub sign_metadata: bool,
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults {
            channel: Channel::Stable,
            keep_versions: 3,
            check_interval_mins: 10,
            metadata_expire: "6h".to_string(),
            sign_metadata: true,
        }
    }
}

fn default_keep() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

/// One tracked project.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    pub id: String,
    pub name: String,
    pub forge: ForgeKind,
    pub host: String,
    pub project: String,
    /// Override of the API base URL (GitHub Enterprise, tests).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
    #[serde(default)]
    pub channel: Channel,
    /// Regex on asset file names, needed when a release ships several variants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_pattern: Option<String>,
    /// Package names this source may provide (dnf `includepkgs`).
    pub packages: Vec<String>,
    #[serde(default = "default_keep")]
    pub keep_versions: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Packages outside this source that its packages may obsolete.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_obsoletes: Vec<String>,
    /// Upstream signing key (URL or file://), enables `gpgcheck=1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_gpgkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_at: Option<String>,
}

impl SourceConfig {
    pub fn forge_ref(&self) -> ForgeRef {
        ForgeRef {
            forge: self.forge,
            host: self.host.clone(),
            project: self.project.clone(),
            api_url: self.api_url.clone(),
        }
    }

    pub fn web_url(&self) -> String {
        self.forge_ref().web_url()
    }

    pub fn asset_regex(&self) -> Result<Option<Regex>> {
        compile_pattern(self.asset_pattern.as_deref())
    }

    pub fn to_view(&self) -> SourceView {
        SourceView {
            id: self.id.clone(),
            name: self.name.clone(),
            forge: self.forge,
            host: self.host.clone(),
            project: self.project.clone(),
            web_url: self.web_url(),
            channel: self.channel,
            asset_pattern: self.asset_pattern.clone(),
            packages: self.packages.clone(),
            keep_versions: self.keep_versions,
            enabled: self.enabled,
            added_at: self.added_at.clone(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err(usage(tr("source name is empty")));
        }
        if self.project.trim().is_empty() || self.host.trim().is_empty() {
            return Err(usage(tr("source project or host is empty")));
        }
        if self.packages.is_empty() {
            return Err(usage(trf("source '{id}' lists no packages", &[("id", &self.id)])));
        }
        for p in &self.packages {
            validate_package_name(p)?;
        }
        if !(1..=20).contains(&self.keep_versions) {
            return Err(usage(tr("keep_versions must be between 1 and 20")));
        }
        self.asset_regex()?;
        Ok(())
    }
}

pub fn compile_pattern(pattern: Option<&str>) -> Result<Option<Regex>> {
    match pattern {
        None => Ok(None),
        Some("") => Ok(None),
        Some(p) => Regex::new(p).map(Some).map_err(|e| usage(trf("invalid asset pattern: {error}", &[("error", &e)]))),
    }
}

/// Source ids end up in file names, repo ids and URLs: keep them boring.
pub fn validate_id(id: &str) -> Result<()> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[a-z0-9][a-z0-9_.-]{0,62}$").unwrap());
    if re.is_match(id) && !id.contains("..") {
        Ok(())
    } else {
        Err(usage(trf(
            "invalid source id '{id}': use lowercase letters, digits, '.', '_' or '-' (max 63)",
            &[("id", &id)],
        )))
    }
}

pub fn validate_package_name(name: &str) -> Result<()> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[A-Za-z0-9_+][A-Za-z0-9_.+-]{0,127}$").unwrap());
    if re.is_match(name) {
        Ok(())
    } else {
        Err(usage(trf("invalid package name '{name}'", &[("name", &name)])))
    }
}

/// Turn any project name into a valid source id.
pub fn sanitize_id(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed: String = out.trim_matches(|c| c == '-' || c == '.').chars().take(63).collect();
    if trimmed.is_empty() {
        "source".to_string()
    } else {
        trimmed
    }
}

pub fn load_global(paths: &Paths) -> Result<GlobalConfig> {
    let path = paths.global_config();
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).with_context(|| trf("parsing {path}", &[("path", &path.display())])),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(GlobalConfig::default()),
        Err(e) => Err(e).with_context(|| trf("reading {path}", &[("path", &path.display())])),
    }
}

pub fn save_global(paths: &Paths, cfg: &GlobalConfig) -> Result<()> {
    let body = toml::to_string_pretty(cfg)?;
    let text = format!("# quai global configuration. See `man quai` or https://github.com/IT-Yohan/quai\n\n{body}");
    write_atomic(&paths.global_config(), text.as_bytes(), 0o644)
}

pub fn load_source(paths: &Paths, id: &str) -> Result<SourceConfig> {
    validate_id(id)?;
    let path = paths.source_file(id);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(not_found(trf("no source named '{id}'", &[("id", &id)])));
        }
        Err(e) => return Err(e).with_context(|| trf("reading {path}", &[("path", &path.display())])),
    };
    let src: SourceConfig =
        toml::from_str(&text).with_context(|| trf("parsing {path}", &[("path", &path.display())]))?;
    if src.id != id {
        return Err(usage(trf(
            "{path} declares id '{declared}' but its file name says '{id}'",
            &[("path", &path.display()), ("declared", &src.id), ("id", &id)],
        )));
    }
    Ok(src)
}

/// All sources, sorted by id. Unreadable files are returned as errors next to the good ones.
pub fn load_sources(paths: &Paths) -> (Vec<SourceConfig>, Vec<(String, anyhow::Error)>) {
    let mut ok = Vec::new();
    let mut bad = Vec::new();
    let Ok(entries) = fs::read_dir(paths.sources_dir()) else {
        return (ok, bad);
    };
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_suffix(".toml").map(|s| s.to_string())
        })
        .collect();
    ids.sort();
    for id in ids {
        match load_source(paths, &id).and_then(|s| s.validate().map(|_| s)) {
            Ok(s) => ok.push(s),
            Err(e) => bad.push((id, e)),
        }
    }
    (ok, bad)
}

pub fn save_source(paths: &Paths, src: &SourceConfig) -> Result<()> {
    src.validate()?;
    let body = toml::to_string_pretty(src)?;
    let text = format!(
        "# quai source '{}'. Edit with `quai set {} ...`, or edit by hand and run `quai sync`.\n\n{body}",
        src.id, src.id
    );
    write_atomic(&paths.source_file(&src.id), text.as_bytes(), 0o644)
}

pub fn remove_source(paths: &Paths, id: &str) -> Result<()> {
    validate_id(id)?;
    let path = paths.source_file(id);
    fs::remove_file(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            not_found(trf("no source named '{id}'", &[("id", &id)]))
        } else {
            anyhow::Error::new(e).context(trf("removing {path}", &[("path", &path.display())]))
        }
    })
}

/// Pick an id that is not taken yet, based on the project name.
pub fn unique_id(paths: &Paths, project: &str) -> String {
    let segments: Vec<&str> = project.split('/').filter(|s| !s.is_empty()).collect();
    let last = segments.last().copied().unwrap_or(project);
    let base = sanitize_id(last);
    if !paths.source_file(&base).exists() {
        return base;
    }
    if segments.len() >= 2 {
        let owner = sanitize_id(segments.get(segments.len() - 2).copied().unwrap_or("x"));
        let candidate = sanitize_id(&format!("{owner}-{base}"));
        if !paths.source_file(&candidate).exists() {
            return candidate;
        }
    }
    for i in 2..1000 {
        let candidate = format!("{base}-{i}");
        if !paths.source_file(&candidate).exists() {
            return candidate;
        }
    }
    base
}

/// Refuse to track the same project twice.
pub fn find_by_project(paths: &Paths, forge: ForgeKind, host: &str, project: &str) -> Option<SourceConfig> {
    let (sources, _) = load_sources(paths);
    sources
        .into_iter()
        .find(|s| s.forge == forge && s.host.eq_ignore_ascii_case(host) && s.project.eq_ignore_ascii_case(project))
}

pub fn ensure_new_id(paths: &Paths, id: &str) -> Result<()> {
    validate_id(id)?;
    if paths.source_file(id).exists() {
        return Err(conflict(trf("a source named '{id}' already exists", &[("id", &id)])));
    }
    Ok(())
}

/// Forge tokens, keyed by forge host (`github.com`, `gitlab.com`, `codeberg.org`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Tokens {
    pub tokens: BTreeMap<String, String>,
}

impl Tokens {
    pub fn get(&self, host: &str) -> Option<&str> {
        self.tokens.get(&host.to_ascii_lowercase()).map(|s| s.as_str())
    }
}

pub fn load_tokens(paths: &Paths) -> Tokens {
    match fs::read_to_string(paths.tokens_file()) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!("ignoring unreadable {}: {e}", paths.tokens_file().display());
            Tokens::default()
        }),
        Err(_) => Tokens::default(),
    }
}

pub fn save_tokens(paths: &Paths, tokens: &Tokens) -> Result<()> {
    ensure_private_dir(paths)?;
    let text = format!(
        "# Forge API tokens used by quaid. Readable by root and the quai group only.\n\n{}",
        toml::to_string_pretty(tokens)?
    );
    let path = paths.tokens_file();
    write_atomic(&path, text.as_bytes(), 0o640)?;
    chown_root_group(&path, SYSTEM_GROUP)
}

/// `/etc/quai/private`, mode 0750 root:quai.
pub fn ensure_private_dir(paths: &Paths) -> Result<()> {
    let dir = paths.private_dir();
    fs::create_dir_all(&dir).with_context(|| trf("creating {path}", &[("path", &dir.display())]))?;
    crate::fsutil::set_mode(&dir, 0o750)?;
    chown_root_group(&dir, SYSTEM_GROUP)
}

/// Portable list of sources for dotfiles (`quai export` / `quai import`).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ExportFile {
    #[serde(default, rename = "source")]
    pub sources: Vec<SourceConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_sanitized_and_validated() {
        assert_eq!(sanitize_id("Orca"), "orca");
        assert_eq!(sanitize_id("My Cool App!"), "my-cool-app");
        assert!(validate_id("orca").is_ok());
        assert!(validate_id("stablyai-orca.v2").is_ok());
        assert!(validate_id("../etc").is_err());
        assert!(validate_id("Orca").is_err());
        assert!(validate_id("").is_err());
    }

    #[test]
    fn source_round_trips_through_toml() {
        let src = SourceConfig {
            id: "orca".into(),
            name: "Orca IDE".into(),
            forge: ForgeKind::Github,
            host: "github.com".into(),
            project: "stablyai/orca".into(),
            api_url: None,
            channel: Channel::Stable,
            asset_pattern: Some("^orca-ide-".into()),
            packages: vec!["orca-ide".into()],
            keep_versions: 3,
            enabled: true,
            allow_obsoletes: vec![],
            package_gpgkey: None,
            added_at: None,
        };
        let text = toml::to_string_pretty(&src).unwrap();
        let back: SourceConfig = toml::from_str(&text).unwrap();
        assert_eq!(src, back);
        assert!(back.validate().is_ok());
    }

    #[test]
    fn packaged_config_matches_the_defaults() {
        let text = include_str!("../../../data/quai.toml");
        let cfg: GlobalConfig = toml::from_str(text).unwrap();
        assert_eq!(cfg, GlobalConfig::default());
    }

    #[test]
    fn global_defaults_parse_from_empty_file() {
        let cfg: GlobalConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.daemon.listen, DEFAULT_LISTEN);
        assert_eq!(cfg.defaults.keep_versions, 3);
    }
}
