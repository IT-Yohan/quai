//! Build-time outputs derived from the command line definition, in English:
//! man pages, shell completions and the --help texts as a gettext template.
//! Used by the `quai-generate` tool.

use clap::builder::StyledStr;
use clap::Command;
use std::collections::BTreeSet;
use std::path::Path;

/// Every translatable help text of `cmd` and its subcommands, built-in
/// `--help`/`--version` texts included, with the command each belongs to.
fn texts(cmd: &Command, path: &str, out: &mut Vec<(String, String)>) {
    let push = |out: &mut Vec<(String, String)>, s: Option<&StyledStr>| {
        if let Some(s) = s.map(|s| s.to_string()).filter(|s| !s.trim().is_empty()) {
            out.push((s, path.to_string()));
        }
    };
    push(out, cmd.get_about());
    push(out, cmd.get_long_about());
    push(out, cmd.get_before_help());
    push(out, cmd.get_after_help());
    for a in cmd.get_arguments() {
        push(out, a.get_help());
        push(out, a.get_long_help());
    }
    for sub in cmd.get_subcommands().filter(|s| !s.is_hide_set()) {
        texts(sub, &format!("{path} {}", sub.get_name()), out);
    }
}

fn po_string(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\t', "\\t");
    if !escaped.contains('\n') {
        return format!("\"{escaped}\"");
    }
    let mut out = String::from("\"\"");
    let mut lines = escaped.split('\n').peekable();
    while let Some(line) = lines.next() {
        let nl = if lines.peek().is_some() { "\\n" } else { "" };
        if !line.is_empty() || !nl.is_empty() {
            out.push_str(&format!("\n\"{line}{nl}\""));
        }
    }
    out
}

/// `quai-generate pot FILE`: the help texts as a translation template, merged
/// into `po/quai.pot` by `make pot`.
pub fn write_pot(mut cmd: Command, file: &Path) -> anyhow::Result<()> {
    cmd.build();
    let mut found = Vec::new();
    texts(&cmd, "quai", &mut found);
    let mut seen = BTreeSet::new();
    let mut pot = String::from("msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\n");
    for (msg, path) in found {
        if seen.insert(msg.clone()) {
            pot.push_str(&format!(
                "#. Translators: help of `{path}`; keep commands, options and values as they are.\n#: crates/quai-cli/src/cli.rs\nmsgid {}\nmsgstr \"\"\n\n",
                po_string(&msg)
            ));
        }
    }
    std::fs::write(file, pot)?;
    Ok(())
}

/// `quai-generate man DIR`: quai.1 and one page per subcommand, quai-add.1…
/// (clap's own `help` subcommands get none).
pub fn write_man_pages(mut cmd: Command, dir: &Path) -> anyhow::Result<()> {
    fn render(cmd: &Command, dir: &Path) -> anyhow::Result<()> {
        let name = cmd.get_display_name().unwrap_or_else(|| cmd.get_name());
        let mut page = Vec::new();
        clap_mangen::Man::new(cmd.clone()).render(&mut page)?;
        std::fs::write(dir.join(format!("{name}.1")), page)?;
        for sub in cmd.get_subcommands().filter(|s| !s.is_hide_set() && s.get_name() != "help") {
            render(sub, dir)?;
        }
        Ok(())
    }
    std::fs::create_dir_all(dir)?;
    cmd.build();
    render(&cmd, dir)
}

/// `quai-generate completions DIR`: quai.bash, _quai (zsh) and quai.fish.
pub fn write_completions(mut cmd: Command, dir: &Path) -> anyhow::Result<()> {
    use clap_complete::{generate_to, Shell};
    std::fs::create_dir_all(dir)?;
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
        generate_to(shell, &mut cmd, "quai", dir)?;
    }
    Ok(())
}
