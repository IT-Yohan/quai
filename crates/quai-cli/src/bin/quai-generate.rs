//! Build-time tool, not installed: writes what derives from `quai`'s command
//! line definition, in English.
//!
//!   quai-generate man DIR           man pages
//!   quai-generate completions DIR   bash, zsh and fish completions
//!   quai-generate pot FILE          the --help texts as a gettext template

#[allow(dead_code)]
#[path = "../cli.rs"]
mod cli;
#[path = "../generate.rs"]
mod generate;

use clap::CommandFactory;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = cli::Cli::command();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["man", dir] => generate::write_man_pages(cmd, Path::new(dir)),
        ["completions", dir] => generate::write_completions(cmd, Path::new(dir)),
        ["pot", file] => generate::write_pot(cmd, Path::new(file)),
        _ => {
            eprintln!("usage: quai-generate man DIR | completions DIR | pot FILE");
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("quai-generate: {e:#}");
        std::process::exit(1);
    }
}
