//! `/etc/yum.repos.d/quai-<id>.repo`, derived from the source TOML files.

use crate::config::{GlobalConfig, SourceConfig};
use crate::fsutil::write_atomic;
use crate::paths::Paths;
use anyhow::{Context, Result};
use std::fs;

/// First line of every file quai generates; files without it are never touched.
pub const MARKER: &str = "# Managed by quai";

pub fn repo_id(id: &str) -> String {
    format!("quai-{id}")
}

pub fn baseurl(global: &GlobalConfig, id: &str) -> String {
    format!("http://{}/repos/{}/", global.daemon.listen, id)
}

/// Render the `.repo` file for one source.
pub fn render(paths: &Paths, global: &GlobalConfig, src: &SourceConfig) -> String {
    let sign = global.defaults.sign_metadata;
    let mut keys = Vec::new();
    if sign {
        keys.push(format!("file://{}", paths.public_key().display()));
    }
    if let Some(k) = &src.package_gpgkey {
        keys.push(k.clone());
    }
    let mut out = String::new();
    out.push_str(MARKER);
    out.push_str(&format!(" from {}. Do not edit: use `quai set {} ...`.\n", paths.source_file(&src.id).display(), src.id));
    out.push_str(&format!("[{}]\n", repo_id(&src.id)));
    out.push_str(&format!("name={} (quai: {})\n", single_line(&src.name), src.forge_ref().display()));
    out.push_str(&format!("baseurl={}\n", baseurl(global, &src.id)));
    out.push_str(&format!("enabled={}\n", u8::from(src.enabled)));
    out.push_str("type=rpm\n");
    out.push_str(&format!("repo_gpgcheck={}\n", u8::from(sign)));
    out.push_str(&format!("gpgcheck={}\n", u8::from(src.package_gpgkey.is_some())));
    if !keys.is_empty() {
        out.push_str(&format!("gpgkey={}\n", keys.join(" ")));
    }
    out.push_str(&format!("includepkgs={}\n", src.packages.join(",")));
    out.push_str(&format!("metadata_expire={}\n", global.defaults.metadata_expire));
    out.push_str("skip_if_unavailable=1\n");
    out
}

fn single_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

/// Is `/etc/yum.repos.d/quai-<id>.repo` exactly what quai would write?
pub fn in_sync(paths: &Paths, global: &GlobalConfig, src: &SourceConfig) -> bool {
    fs::read_to_string(paths.repo_file(&src.id)).map(|t| t == render(paths, global, src)).unwrap_or(false)
}

pub fn write(paths: &Paths, global: &GlobalConfig, src: &SourceConfig) -> Result<bool> {
    let body = render(paths, global, src);
    let path = paths.repo_file(&src.id);
    if fs::read_to_string(&path).map(|t| t == body).unwrap_or(false) {
        return Ok(false);
    }
    write_atomic(&path, body.as_bytes(), 0o644)?;
    Ok(true)
}

/// Remove `quai-<id>.repo` if quai generated it.
pub fn remove(paths: &Paths, id: &str) -> Result<bool> {
    let path = paths.repo_file(id);
    match fs::read_to_string(&path) {
        Ok(text) if text.starts_with(MARKER) => {
            fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Ids of every `.repo` file quai generated.
pub fn managed_ids(paths: &Paths) -> Vec<String> {
    let Ok(entries) = fs::read_dir(&paths.repos_dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let id = name.strip_prefix("quai-")?.strip_suffix(".repo")?.to_string();
            let text = fs::read_to_string(e.path()).ok()?;
            text.starts_with(MARKER).then_some(id)
        })
        .collect();
    ids.sort();
    ids
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    pub unchanged: Vec<String>,
}

/// Make `/etc/yum.repos.d` match the sources exactly.
pub fn sync(paths: &Paths, global: &GlobalConfig, sources: &[SourceConfig]) -> Result<SyncReport> {
    let mut report = SyncReport::default();
    for src in sources {
        if write(paths, global, src)? {
            report.written.push(src.id.clone());
        } else {
            report.unchanged.push(src.id.clone());
        }
    }
    for id in managed_ids(paths) {
        if !sources.iter().any(|s| s.id == id) && remove(paths, &id)? {
            report.removed.push(id);
        }
    }
    Ok(report)
}
