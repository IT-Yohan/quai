//! `quai`: command-line administration of quai sources.
//!
//! Commands that change the system need root (`sudo quai ...`, or `pkexec quai ...`
//! from the graphical frontend). Every command accepts `--json`.

mod admin;
mod cli;
mod ctx;
mod help;
mod output;
mod sources;
mod status;

use clap::{CommandFactory, FromArgMatches};
pub use cli::Selection;
use cli::{Cli, Cmd, TokenCmd};
use ctx::Ctx;
use output::Output;
use quai_i18n::trf;
use std::path::PathBuf;

fn run(cli: Cli) -> anyhow::Result<Output> {
    let ctx = Ctx::new(cli.json, cli.yes)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    match cli.cmd {
        Cmd::Preview { sel } => rt.block_on(sources::preview(&ctx, &sel)),
        Cmd::Add { sel, id, name, keep, install } => rt.block_on(sources::add(&ctx, &sel, id, name, keep, install)),
        Cmd::Install { id } => sources::install(&ctx, &id),
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
            (Some(k), None) => Err(quai_core::error::usage(trf("missing value for {key}", &[("key", &k)]))),
        },
        Cmd::Doctor => rt.block_on(status::doctor(&ctx)),
        Cmd::Gui { .. } => unreachable!("handled in main"),
    }
}

/// `quai gui`: replace this process with the frontend that fits the desktop,
/// looked up next to `quai` first (development builds), then in `$PATH`.
fn launch_gui(frontend: Option<String>) -> anyhow::Error {
    use std::os::unix::process::CommandExt;
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let requested = frontend.or_else(|| std::env::var("QUAI_GUI").ok());
    let candidates = match quai_types::frontend::gui_candidates(&desktop, requested.as_deref()) {
        Ok(c) => c,
        Err(e) => return quai_core::error::usage(e),
    };
    let mut dirs: Vec<PathBuf> =
        std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).into_iter().collect();
    dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin".into())));
    for name in &candidates {
        if let Some(bin) = dirs.iter().map(|d| d.join(name)).find(|p| p.is_file()) {
            let err = std::process::Command::new(&bin).exec();
            return anyhow::anyhow!(trf(
                "cannot start {program}: {error}",
                &[("program", &bin.display()), ("error", &err)]
            ));
        }
    }
    anyhow::anyhow!(help::no_gui_message(&candidates))
}

fn main() {
    quai_i18n::init();
    let matches = help::localized(Cli::command()).get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let json = cli.json;
    if let Cmd::Gui { frontend } = cli.cmd {
        fail(&launch_gui(frontend), json);
    }
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
        Err(e) => fail(&e, json),
    }
}

fn fail(e: &anyhow::Error, json: bool) -> ! {
    output::print_error(e, json);
    std::process::exit(match quai_core::error::kind_of(e) {
        quai_core::Kind::Usage => 2,
        quai_core::Kind::Permission => 4,
        _ => 1,
    });
}
