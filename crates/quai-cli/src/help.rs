//! `--help` in the user's language.
//!
//! The help texts are the doc comments of `cli.rs`, in English. At run time
//! [`localized`] looks each of them up in the catalogue; `quai-generate pot`
//! (see `generate.rs`) lists them so `make pot` adds them to `po/quai.pot`.

use clap::builder::StyledStr;
use clap::{Arg, Command};
use quai_i18n::{tr, trf};

fn translate(s: Option<&StyledStr>) -> Option<String> {
    s.map(|s| tr(&s.to_string()))
}

fn localize_arg(a: Arg) -> Arg {
    let heading = if a.is_positional() { tr("Arguments") } else { tr("Options") };
    let mut a = a.help_heading(heading);
    if let Some(h) = translate(a.get_help()) {
        a = a.help(h);
    }
    if let Some(h) = translate(a.get_long_help()) {
        a = a.long_help(h);
    }
    a
}

fn localize_command(mut cmd: Command) -> Command {
    if let Some(s) = translate(cmd.get_about()) {
        cmd = cmd.about(s);
    }
    if let Some(s) = translate(cmd.get_long_about()) {
        cmd = cmd.long_about(s);
    }
    if let Some(s) = translate(cmd.get_after_help()) {
        cmd = cmd.after_help(s);
    }
    let usage = cmd.get_styles().get_usage();
    // Translators: the heading in front of a command's synopsis in --help.
    let heading = format!("{}{}{}", usage.render(), tr("Usage:"), usage.render_reset());
    cmd = cmd
        .help_template(format!(
            "{{before-help}}{{about-with-newline}}\n{heading} {{usage}}\n\n{{all-args}}{{after-help}}"
        ))
        .subcommand_help_heading(tr("Commands"));
    let args: Vec<String> = cmd.get_arguments().map(|a| a.get_id().to_string()).collect();
    for id in args {
        cmd = cmd.mut_arg(id, localize_arg);
    }
    let subs: Vec<String> = cmd.get_subcommands().map(|s| s.get_name().to_string()).collect();
    for name in subs {
        cmd = cmd.mut_subcommand(name, localize_command);
    }
    cmd
}

/// The command line definition with its help in the user's language.
pub fn localized(mut cmd: Command) -> Command {
    // Build first, so the --help, --version and `help` entries clap adds exist.
    cmd.build();
    localize_command(cmd)
}

/// Shown when `quai gui` finds no frontend.
pub fn no_gui_message(candidates: &[&str]) -> String {
    trf(
        "no graphical interface is installed: install {packages} (sudo dnf install {first})",
        &[("packages", &candidates.join(&format!(" {} ", tr("or")))), ("first", &candidates[0])],
    )
}
