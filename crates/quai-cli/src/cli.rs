//! The command line of `quai`, shared with the build-time `quai-generate`
//! (man pages, completions, translation template of the help).

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "quai",
    version,
    about = "Track RPM packages published as GitHub, GitLab or Forgejo releases, through dnf.",
    after_help = "Examples:\n  quai preview stablyai/orca\n  sudo quai add stablyai/orca --install\n  quai status\n  sudo quai upgrade orca"
)]
pub struct Cli {
    /// Print one JSON document on stdout (used by the graphical interfaces).
    #[arg(long, global = true)]
    pub json: bool,
    /// Answer yes to confirmations.
    #[arg(short = 'y', long, global = true)]
    pub yes: bool,
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(clap::Args, Clone)]
pub struct Selection {
    /// owner/repo, a forge URL, or forge:host/owner/repo (codeberg:, gitlab:, forgejo:)
    pub spec: String,
    /// Forge type for hosts quai does not know: github, gitlab or forgejo.
    #[arg(long)]
    pub forge: Option<String>,
    /// Follow stable releases only (default) or pre-releases too.
    #[arg(long, value_parser = ["stable", "prerelease"])]
    pub channel: Option<String>,
    /// Regex on asset file names, when a release ships several RPM variants.
    #[arg(long = "asset-pattern")]
    pub asset_pattern: Option<String>,
    /// API base URL override (GitHub Enterprise; tests).
    #[arg(long = "api-url", hide = true)]
    pub api_url: Option<String>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Show what `add` would do, without changing anything.
    Preview {
        #[command(flatten)]
        sel: Selection,
    },
    /// Start tracking a project (root).
    Add {
        #[command(flatten)]
        sel: Selection,
        /// Source id (default: the project name).
        #[arg(long)]
        id: Option<String>,
        /// Display name.
        #[arg(long)]
        name: Option<String>,
        /// Number of versions kept available for downgrades.
        #[arg(long)]
        keep: Option<u32>,
        /// Install the packages right away.
        #[arg(long)]
        install: bool,
    },
    /// Install the packages of a tracked source (root).
    Install { id: String },
    /// Stop tracking a source (root).
    Remove {
        id: String,
        /// Also uninstall its packages.
        #[arg(long)]
        uninstall: bool,
    },
    /// List tracked sources.
    List,
    /// Installed versus available versions.
    Status { id: Option<String> },
    /// Ask quaid to check the forges now.
    Refresh { id: Option<String> },
    /// Upgrade the packages of one source, or of all (root).
    Upgrade { id: Option<String> },
    /// Enable a source (root).
    Enable { id: String },
    /// Disable a source without removing it (root).
    Disable { id: String },
    /// Change a setting of a source (root).
    ///
    /// Keys: name, channel, asset-pattern, keep-versions, allow-package,
    /// disallow-package, allow-obsoletes, disallow-obsoletes, package-gpgkey.
    /// An empty value clears asset-pattern and package-gpgkey.
    Set { id: String, key: String, value: String },
    /// Regenerate /etc/yum.repos.d/quai-*.repo from the source files (root).
    Sync,
    /// Write all sources to one TOML file, for dotfiles.
    Export {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Add every source from an exported file (root).
    Import {
        file: PathBuf,
        /// Install the packages too.
        #[arg(long)]
        install: bool,
        /// Replace sources that already exist.
        #[arg(long)]
        replace: bool,
    },
    /// Create the signing key, directories and enable quaid.socket (root).
    Setup {
        /// Do not touch systemd units.
        #[arg(long)]
        no_systemd: bool,
    },
    /// Manage forge API tokens (root).
    Token {
        #[command(subcommand)]
        action: TokenCmd,
    },
    /// Show global settings, or change one (root): listen, check-interval-mins,
    /// keep-versions, metadata-expire, sign-metadata.
    Config { key: Option<String>, value: Option<String> },
    /// Check the whole setup and explain how to fix problems.
    Doctor,
    /// Open the graphical interface that fits the desktop: Kirigami on KDE
    /// Plasma, GTK/libadwaita elsewhere, or whichever one is installed.
    Gui {
        /// Use this frontend instead of choosing one (also $QUAI_GUI).
        #[arg(long, value_parser = ["gtk", "kde"])]
        frontend: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum TokenCmd {
    /// Store a token for a forge host; the token is read from stdin.
    Set {
        #[arg(long, default_value = "github.com")]
        host: String,
    },
    /// Forget the token of a host.
    Clear {
        #[arg(long, default_value = "github.com")]
        host: String,
    },
    /// Hosts that have a token.
    List,
}
