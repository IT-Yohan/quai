//! JSON contract between `quai` (CLI) and `quai-gtk` (frontend).
//!
//! Every `quai <command> --json` prints exactly one of these documents on
//! stdout. Failures print an [`ErrorReport`] and exit with a non-zero code.

use serde::{Deserialize, Serialize};

/// Kind of code forge a source lives on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ForgeKind {
    Github,
    Gitlab,
    Forgejo,
}

impl ForgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ForgeKind::Github => "github",
            ForgeKind::Gitlab => "gitlab",
            ForgeKind::Forgejo => "forgejo",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "github" => Some(ForgeKind::Github),
            "gitlab" => Some(ForgeKind::Gitlab),
            "forgejo" | "gitea" | "codeberg" => Some(ForgeKind::Forgejo),
            _ => None,
        }
    }
}

/// Which releases a source follows.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Prerelease,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Prerelease => "prerelease",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "stable" => Some(Channel::Stable),
            "prerelease" | "pre-release" | "beta" => Some(Channel::Prerelease),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Danger,
}

/// Something the user should know before or after adding a source.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// Stable machine-readable code, e.g. `unsigned_packages`.
    pub code: String,
    pub severity: Severity,
    pub message: String,
}

/// Where the SHA-256 that dnf will verify comes from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChecksumSource {
    /// Published by the forge (GitHub asset digests).
    Forge,
    /// Computed by quaid by downloading the asset once.
    Computed,
    /// Will be computed on the first refresh.
    Pending,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: Option<String>,
    pub published_at: Option<String>,
    pub prerelease: bool,
}

/// One package found in the newest matching release.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PreviewPackage {
    pub name: String,
    /// `epoch:version-release`, epoch omitted when zero.
    pub evr: String,
    pub arch: String,
    pub summary: String,
    pub size: Option<u64>,
    pub asset_name: String,
    pub checksum: ChecksumSource,
    /// Installed EVR, if the package is already on the system.
    pub installed: Option<String>,
    /// Another enabled repository that already ships a package with this name.
    pub other_repo: Option<String>,
}

/// Several assets provide the same package name for the same architecture:
/// the user has to pick one (this becomes the source's asset pattern).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct VariantGroup {
    pub name: String,
    pub arch: String,
    pub assets: Vec<String>,
}

/// What `quai add` would do, computed without root.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub forge: ForgeKind,
    pub host: String,
    pub project: String,
    pub web_url: String,
    pub suggested_id: String,
    pub suggested_name: String,
    pub release: ReleaseInfo,
    pub packages: Vec<PreviewPackage>,
    pub variants: Vec<VariantGroup>,
    pub warnings: Vec<Warning>,
    pub asset_pattern: Option<String>,
    /// Native architecture of this machine (`x86_64`, `aarch64`...).
    pub host_arch: String,
}

/// A configured source, as shown to users.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SourceView {
    pub id: String,
    pub name: String,
    pub forge: ForgeKind,
    pub host: String,
    pub project: String,
    pub web_url: String,
    pub channel: Channel,
    pub asset_pattern: Option<String>,
    pub packages: Vec<String>,
    pub keep_versions: u32,
    pub enabled: bool,
    pub added_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    UpToDate,
    UpdateAvailable,
    NotInstalled,
    NeverChecked,
    Error,
    Disabled,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PackageState {
    pub name: String,
    pub arch: Option<String>,
    /// Newest EVR served by quaid for this machine's architecture.
    pub available: Option<String>,
    pub installed: Option<String>,
    pub update_available: bool,
}

/// A package that quaid refused to serve, and why.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Excluded {
    pub name: String,
    pub file: String,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SourceStatus {
    pub source: SourceView,
    pub state: SourceState,
    pub packages: Vec<PackageState>,
    pub last_check: Option<String>,
    pub last_error: Option<String>,
    pub latest_release: Option<String>,
    pub excluded: Vec<Excluded>,
    pub repo_file_in_sync: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StatusReport {
    pub sources: Vec<SourceStatus>,
    pub daemon_reachable: bool,
    pub atomic: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DoctorCheck {
    pub id: String,
    pub title: String,
    pub ok: bool,
    pub detail: String,
    /// Command or action that fixes the problem.
    pub fix: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DoctorReport {
    pub ok: bool,
    pub checks: Vec<DoctorCheck>,
}

/// Result of a command that changes the system.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub source: Option<SourceView>,
    /// Extra lines (commands run, notes such as "reboot required").
    pub details: Vec<String>,
    pub reboot_required: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TokenInfo {
    pub host: String,
    pub configured: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub listen: String,
    pub check_interval_mins: u32,
    pub keep_versions: u32,
    pub metadata_expire: String,
    pub sign_metadata: bool,
    pub tokens: Vec<TokenInfo>,
}

/// Printed instead of the expected document when a command fails.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ErrorReport {
    pub error: String,
    /// Short category: `usage`, `permission`, `network`, `not_found`, `conflict`, `internal`.
    pub kind: String,
}

/// Reply of `quaid`'s `POST /-/refresh` endpoint.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RefreshReply {
    pub refreshed: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_round_trip_as_lowercase() {
        assert_eq!(serde_json::to_string(&ForgeKind::Github).unwrap(), "\"github\"");
        assert_eq!(serde_json::to_string(&SourceState::UpdateAvailable).unwrap(), "\"update_available\"");
        assert_eq!(ForgeKind::parse("Codeberg"), Some(ForgeKind::Forgejo));
        assert_eq!(Channel::parse("beta"), Some(Channel::Prerelease));
    }
}
