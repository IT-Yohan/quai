//! Questions asked to the running system: rpm database, dnf, ostree, privileges.

use anyhow::{Context, Result};
use quai_i18n::trf;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

/// Fedora Atomic (Silverblue, Kinoite, ...) is booted through ostree.
pub fn is_atomic() -> bool {
    Path::new("/run/ostree-booted").exists()
}

/// rpm's name for the native architecture.
pub fn host_arch() -> String {
    match std::env::consts::ARCH {
        "x86" => "i686".to_string(),
        "powerpc64" if cfg!(target_endian = "little") => "ppc64le".to_string(),
        other => other.to_string(),
    }
}

/// Can a package of `arch` be installed here?
pub fn arch_compatible(arch: &str, host: &str) -> bool {
    arch == "noarch" || arch == host || (host == "x86_64" && matches!(arch, "i686" | "i586" | "i386"))
}

pub fn which(binary: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_else(|| "/usr/sbin:/usr/bin:/sbin:/bin".into());
    std::env::split_paths(&path).map(|d| d.join(binary)).find(|p| p.is_file())
}

/// The dnf front-end to call (`dnf` is dnf5 on Fedora 41+).
pub fn dnf_command() -> Option<&'static str> {
    if which("dnf").is_some() {
        Some("dnf")
    } else if which("dnf5").is_some() {
        Some("dnf5")
    } else {
        None
    }
}

/// Extra arguments for every dnf call, from `QUAI_DNF_ARGS` (whitespace separated).
///
/// Meant for development and tests, e.g.
/// `--installroot=/tmp/root --setopt=reposdir=/tmp/repos.d --releasever=43`.
pub fn dnf_extra_args() -> Vec<String> {
    std::env::var("QUAI_DNF_ARGS").map(|v| v.split_whitespace().map(|s| s.to_string()).collect()).unwrap_or_default()
}

/// A `dnf` command with [`dnf_extra_args`] applied, or `None` when dnf is absent.
pub fn dnf() -> Option<Command> {
    let bin = dnf_command()?;
    let mut cmd = Command::new(bin);
    cmd.args(dnf_extra_args());
    Some(cmd)
}

/// Installed EVRs per package name, from the rpm database (`QUAI_RPM_ROOT` for a chroot).
pub fn installed_versions(names: &[String]) -> BTreeMap<String, Vec<(String, String)>> {
    let mut out: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    if names.is_empty() || which("rpm").is_none() {
        return out;
    }
    let mut cmd = Command::new("rpm");
    if let Ok(root) = std::env::var("QUAI_RPM_ROOT") {
        cmd.arg("--root").arg(root);
    }
    let output = cmd
        .arg("-q")
        .arg("--qf")
        .arg("%{NAME}\t%{EPOCHNUM}:%{VERSION}-%{RELEASE}\t%{ARCH}\n")
        .args(names)
        .stderr(Stdio::null())
        .output();
    let Ok(output) = output else {
        return out;
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 3 {
            continue; // "package foo is not installed"
        }
        let evr = parts[1].strip_prefix("0:").unwrap_or(parts[1]).to_string();
        out.entry(parts[0].to_string()).or_default().push((evr, parts[2].to_string()));
    }
    out
}

/// A repository other than quai's that already offers `name`, using dnf's cache only.
pub fn other_repo_providing(name: &str) -> Option<String> {
    let output = dnf()?
        .args(["repoquery", "--cacheonly", "--quiet", "--available", "--qf", "%{name} %{repoid}\\n", name])
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter(|(n, repo)| *n == name && !repo.starts_with("quai-") && *repo != "@System")
        .map(|(_, repo)| repo.to_string())
        .next()
}

/// Run a command with inherited stdio (dnf asks questions, shows progress).
pub fn run_interactive(cmd: &mut Command) -> Result<()> {
    let desc = format!("{cmd:?}");
    let status = cmd.status().with_context(|| trf("running {command}", &[("command", &desc)]))?;
    if !status.success() {
        anyhow::bail!(trf("{command} failed ({status})", &[("command", &desc), ("status", &status)]));
    }
    Ok(())
}

/// Run a command, capturing output; stderr is returned in the error.
pub fn run_captured(cmd: &mut Command) -> Result<String> {
    let desc = format!("{cmd:?}");
    let output = cmd.stdin(Stdio::null()).output().with_context(|| trf("running {command}", &[("command", &desc)]))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = err.lines().rev().take(6).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        anyhow::bail!(trf(
            "{command} failed ({status}): {output}",
            &[("command", &desc), ("status", &output.status), ("output", &tail.join(" | "))],
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn systemctl(args: &[&str]) -> Result<String> {
    run_captured(Command::new("systemctl").args(args))
}

pub fn has_systemd() -> bool {
    Path::new("/run/systemd/system").exists() && which("systemctl").is_some()
}

pub fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "localhost".to_string())
}
