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
    /// One `--asset-pattern` per asset, matching it whatever the version.
    pub patterns: Vec<String>,
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
    /// Why, in words. quaid records it in the system language; `quai status`
    /// writes it again in the user's from `cause`.
    pub reason: String,
    /// The same reason as data (absent in state written by quai 0.1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<ExclusionCause>,
}

/// Why quaid does not serve a package of a release.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExclusionCause {
    /// The package is not in the source's `packages` list.
    NotListed,
    /// The package obsoletes `obsoleted`, which the source does not own.
    Obsoletes { obsoleted: String },
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

/// Helpers shared by the graphical frontends (GTK and KDE), which both drive
/// `quai --json` as a subprocess. Standard library only.
pub mod frontend {
    use super::ErrorReport;
    use quai_i18n::{tr, trf};

    /// The `quai` binary: `$QUAI_BIN`, else the first one in `$PATH`, else `/usr/bin/quai`.
    pub fn quai_binary() -> String {
        if let Ok(p) = std::env::var("QUAI_BIN") {
            return p;
        }
        let path = std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into());
        std::env::split_paths(&path)
            .map(|d| d.join("quai"))
            .find(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/usr/bin/quai".to_string())
    }

    /// Command line for `quai <args> --json`, wrapped in `pkexec` for privileged
    /// commands (unless already root or `QUAI_GUI_NO_PKEXEC` is set).
    /// Returns the argv and whether pkexec is used.
    pub fn command_line(args: &[String], privileged: bool, is_root: bool) -> (Vec<String>, bool) {
        let no_pkexec =
            std::env::var_os("QUAI_GUI_NO_PKEXEC").is_some() || std::env::var_os("QUAI_GTK_NO_PKEXEC").is_some();
        let use_pkexec = privileged && !is_root && !no_pkexec;
        let mut argv = Vec::with_capacity(args.len() + 4);
        if use_pkexec {
            argv.push("pkexec".to_string());
        }
        argv.push(quai_binary());
        argv.extend(args.iter().cloned());
        argv.push("--json".to_string());
        if privileged {
            argv.push("--yes".to_string());
        }
        (argv, use_pkexec)
    }

    /// Turn the output of `quai --json` into the JSON document or an error message.
    ///
    /// Commands such as `doctor` exit non-zero while still printing their document,
    /// so the document wins over the exit code unless it is an [`ErrorReport`].
    pub fn interpret(stdout: &str, stderr: &str, exit_code: i32, used_pkexec: bool) -> Result<String, String> {
        if used_pkexec && (exit_code == 126 || exit_code == 127) && stdout.trim().is_empty() {
            return Err(tr("Authentication was cancelled or refused."));
        }
        if let Ok(report) = serde_json::from_str::<ErrorReport>(stdout) {
            return Err(report.error);
        }
        if serde_json::from_str::<serde_json::Value>(stdout).is_ok() {
            return Ok(stdout.to_string());
        }
        let msg = stderr.trim();
        Err(if msg.is_empty() {
            trf("quai failed (exit code {code})", &[("code", &exit_code)])
        } else {
            msg.to_string()
        })
    }

    /// The graphical frontends, in the order `quai gui` tries them.
    ///
    /// `requested` (`gtk` or `kde`, from `--frontend` or `$QUAI_GUI`) pins one.
    /// Otherwise the Kirigami frontend comes first on KDE Plasma and the
    /// libadwaita one everywhere else; the other is the fallback when only one
    /// is installed. `current_desktop` is `$XDG_CURRENT_DESKTOP`.
    pub fn gui_candidates(current_desktop: &str, requested: Option<&str>) -> Result<Vec<&'static str>, String> {
        match requested.map(str::trim).filter(|r| !r.is_empty()) {
            Some("gtk") => return Ok(vec!["quai-gtk"]),
            Some("kde") => return Ok(vec!["quai-kde"]),
            Some(other) => return Err(trf("unknown frontend '{name}' (expected gtk or kde)", &[("name", &other)])),
            None => {}
        }
        let plasma = current_desktop.split(':').any(|d| d.eq_ignore_ascii_case("KDE"));
        Ok(if plasma { vec!["quai-kde", "quai-gtk"] } else { vec!["quai-gtk", "quai-kde"] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_interpretation() {
        use frontend::interpret;
        assert_eq!(interpret("{\"a\":1}", "", 0, false), Ok("{\"a\":1}".into()));
        assert_eq!(interpret("{\"ok\":false}", "", 1, false), Ok("{\"ok\":false}".into()));
        assert_eq!(interpret("{\"error\":\"boom\",\"kind\":\"usage\"}", "", 2, false), Err("boom".into()));
        assert!(interpret("", "", 126, true).unwrap_err().contains("cancelled"));
        assert_eq!(interpret("", "oops\n", 1, false), Err("oops".into()));
        let (argv, pk) = frontend::command_line(&["add".into()], true, false);
        assert!(pk && argv[0] == "pkexec" && argv.ends_with(&["--json".to_string(), "--yes".to_string()]));
        let (argv, pk) = frontend::command_line(&["status".into()], false, false);
        assert!(!pk && argv.last().unwrap() == "--json");
    }

    #[test]
    fn gui_follows_the_desktop() {
        use frontend::gui_candidates;
        assert_eq!(gui_candidates("KDE", None).unwrap(), ["quai-kde", "quai-gtk"]);
        assert_eq!(gui_candidates("ubuntu:GNOME", None).unwrap(), ["quai-gtk", "quai-kde"]);
        assert_eq!(gui_candidates("XFCE", None).unwrap(), ["quai-gtk", "quai-kde"]);
        assert_eq!(gui_candidates("", None).unwrap(), ["quai-gtk", "quai-kde"]);
        assert_eq!(gui_candidates("KDE", Some("gtk")).unwrap(), ["quai-gtk"]);
        assert_eq!(gui_candidates("GNOME", Some(" kde ")).unwrap(), ["quai-kde"]);
        assert_eq!(gui_candidates("GNOME", Some("")).unwrap(), ["quai-gtk", "quai-kde"]);
        assert!(gui_candidates("GNOME", Some("qt")).is_err());
    }

    #[test]
    fn enums_round_trip_as_lowercase() {
        assert_eq!(serde_json::to_string(&ForgeKind::Github).unwrap(), "\"github\"");
        assert_eq!(serde_json::to_string(&SourceState::UpdateAvailable).unwrap(), "\"update_available\"");
        assert_eq!(ForgeKind::parse("Codeberg"), Some(ForgeKind::Forgejo));
        assert_eq!(Channel::parse("beta"), Some(Channel::Prerelease));
    }
}
