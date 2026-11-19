//! `quai`: command-line administration of quai sources.
//!
//! Commands that change the system need root (`sudo quai ...`, or `pkexec quai ...`
//! from the graphical frontend). Every command accepts `--json`.

mod admin;
mod ctx;
mod output;
mod sources;
mod status;

use clap::{Parser, Subcommand};
use ctx::Ctx;
use output::Output;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "quai",
    version,
    about = "Track RPM packages published as GitHub, GitLab or Forgejo releases, through dnf.",
    after_help = "Examples:\n  quai preview stablyai/orca\n  sudo quai add stablyai/orca --install\n  quai status\n  sudo quai upgrade orca"
)]
struct Cli {
    /// Print one JSON document on stdout (used by quai-gtk).
    #[arg(long, global = true)]
    json: bool,
    /// Answer yes to confirmations.
    #[arg(short = 'y', long, global = true)]
    yes: bool,
    #[command(subcommand)]
    cmd: Cmd,
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
enum Cmd {
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
}

#[derive(Subcommand)]
enum TokenCmd {
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

fn run(cli: Cli) -> anyhow::Result<Output> {
    let ctx = Ctx::new(cli.json, cli.yes)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    match cli.cmd {
        Cmd::Preview { sel } => rt.block_on(sources::preview(&ctx, &sel)),
        Cmd::Add { sel, id, name, keep, install } => rt.block_on(sources::add(&ctx, &sel, id, name, keep, install)),
        Cmd::Remove { id, uninstall } => sources::remove(&ctx, &id, uninstall),
        Cmd::List => status::list(&ctx),
        Cmd::Status { id } => rt.block_on(status::status(&ctx, id.as_deref())),
        Cmd::Refresh { id } => rt.block_on(admin::refresh(&ctx, id.as_deref())),
        Cmd::Upgrade { id } => rt.block_on(admin::upgrade(&ctx, id.as_deref())),
        Cmd::Enable { id } => rt.block_on(sources::set_enabled(&ctx, &id, true)),
        Cmd::Disable { id } => rt.block_on(sources::set_enabled(&ctx, &id, false)),
        Cmd::Set { id, key, value } => rt.block_on(sources::set(&ctx, &id, &key, &value)),
        Cmd::Sync => admin::sync(&ctx),
        Cmd::Export { output } => sources::export(&ctx, output.as_deref()),
        Cmd::Import { file, install, replace } => rt.block_on(sources::import(&ctx, &file, install, replace)),
        Cmd::Setup { no_systemd } => admin::setup(&ctx, !no_systemd),
        Cmd::Token { action } => match action {
            TokenCmd::Set { host } => admin::token_set(&ctx, &host),
            TokenCmd::Clear { host } => admin::token_clear(&ctx, &host),
            TokenCmd::List => admin::settings(&ctx),
        },
        Cmd::Config { key, value } => match (key, value) {
            (None, _) => admin::settings(&ctx),
            (Some(k), Some(v)) => admin::config_set(&ctx, &k, &v),
            (Some(k), None) => Err(quai_core::error::usage(format!("missing value for {k}"))),
        },
        Cmd::Doctor => rt.block_on(status::doctor(&ctx)),
    }
}

fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    let filter = tracing_subscriber::EnvFilter::try_from_env("QUAI_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).without_time().init();
    match run(cli) {
        Ok(out) => {
            let failed = out.failed();
            output::print(&out, json);
            if failed {
                std::process::exit(1);
            }
        }
        Err(e) => {
            output::print_error(&e, json);
            std::process::exit(match quai_core::error::kind_of(&e) {
                quai_core::Kind::Usage => 2,
                quai_core::Kind::Permission => 4,
                _ => 1,
            });
        }
    }
}
