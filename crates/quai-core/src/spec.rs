//! Parse what users type: `owner/repo`, `github:owner/repo`, `codeberg:o/r`,
//! `forgejo:git.example.org/o/r`, or any forge URL.

use crate::error::usage;
use crate::forge::ForgeRef;
use anyhow::Result;
use quai_types::ForgeKind;

/// Hosts whose forge type is known without asking.
pub fn known_forge(host: &str) -> Option<ForgeKind> {
    match host.to_ascii_lowercase().as_str() {
        "github.com" | "www.github.com" => Some(ForgeKind::Github),
        "gitlab.com" | "www.gitlab.com" | "gitlab.gnome.org" | "gitlab.freedesktop.org" => {
            Some(ForgeKind::Gitlab)
        }
        "codeberg.org" | "www.codeberg.org" => Some(ForgeKind::Forgejo),
        _ => None,
    }
}

pub fn default_host(forge: ForgeKind) -> &'static str {
    match forge {
        ForgeKind::Github => "github.com",
        ForgeKind::Gitlab => "gitlab.com",
        ForgeKind::Forgejo => "codeberg.org",
    }
}

/// Parse a source specification. `forge_override` is required for unknown hosts.
pub fn parse(input: &str, forge_override: Option<ForgeKind>) -> Result<ForgeRef> {
    let s = input.trim();
    if s.is_empty() {
        return Err(usage("empty source; expected owner/repo or a URL"));
    }

    // URL form.
    if let Some(rest) = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")) {
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = host.to_ascii_lowercase();
        let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
        let forge = match forge_override.or_else(|| known_forge(&host)) {
            Some(f) => f,
            None => {
                return Err(usage(format!(
                    "unknown forge at {host}: pass --forge github, gitlab or forgejo"
                )))
            }
        };
        let project = project_from_path(forge, path)?;
        return Ok(ForgeRef { forge, host, project, api_url: None });
    }

    // prefix:path form.
    if let Some((prefix, rest)) = s.split_once(':') {
        let prefix_l = prefix.to_ascii_lowercase();
        let (forge, default) = match prefix_l.as_str() {
            "github" | "gh" => (ForgeKind::Github, "github.com"),
            "gitlab" | "gl" => (ForgeKind::Gitlab, "gitlab.com"),
            "codeberg" | "cb" => (ForgeKind::Forgejo, "codeberg.org"),
            "forgejo" | "gitea" => (ForgeKind::Forgejo, ""),
            _ => return Err(usage(format!("unknown prefix '{prefix}:' in '{s}'"))),
        };
        let rest = rest.trim_matches('/');
        // An explicit host is recognised by a dot in the first segment.
        let (host, path) = match rest.split_once('/') {
            Some((first, tail)) if first.contains('.') => (first.to_ascii_lowercase(), tail.to_string()),
            _ => (default.to_string(), rest.to_string()),
        };
        if host.is_empty() {
            return Err(usage(format!("{prefix}: needs a host, e.g. {prefix}:git.example.org/owner/repo")));
        }
        let project = project_from_path(forge, &path)?;
        return Ok(ForgeRef { forge, host, project, api_url: None });
    }

    // Bare owner/repo means GitHub, unless a forge was forced.
    let forge = forge_override.unwrap_or(ForgeKind::Github);
    let project = project_from_path(forge, s)?;
    Ok(ForgeRef { forge, host: default_host(forge).to_string(), project, api_url: None })
}

fn project_from_path(forge: ForgeKind, path: &str) -> Result<String> {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let path = path.trim_matches('/');
    // GitLab separates the project from its sub-pages with "/-/".
    let path = path.split("/-/").next().unwrap_or(path);
    let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if let Some(last) = segments.last_mut() {
        if let Some(stripped) = last.strip_suffix(".git") {
            *last = stripped;
        }
    }
    let project_segments: Vec<&str> = match forge {
        // owner/repo, then anything (releases/latest, tree/main...) is ignored.
        ForgeKind::Github | ForgeKind::Forgejo => segments.into_iter().take(2).collect(),
        ForgeKind::Gitlab => {
            // Drop well-known trailing pages when "/-/" was not used.
            let cut = segments
                .iter()
                .position(|s| matches!(*s, "releases" | "tags" | "tree" | "blob"))
                .unwrap_or(segments.len());
            segments.into_iter().take(cut).collect()
        }
    };
    let ok = project_segments.len() >= 2
        && project_segments.iter().all(|s| {
            s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && *s != "." && *s != ".."
        });
    if !ok {
        return Err(usage(format!("cannot read a project from '{path}': expected owner/repo")));
    }
    Ok(project_segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> ForgeRef {
        parse(s, None).unwrap()
    }

    #[test]
    fn bare_owner_repo_is_github() {
        let r = p("stablyai/orca");
        assert_eq!((r.forge, r.host.as_str(), r.project.as_str()), (ForgeKind::Github, "github.com", "stablyai/orca"));
    }

    #[test]
    fn urls_are_understood() {
        let r = p("https://github.com/stablyai/orca/releases/latest");
        assert_eq!(r.project, "stablyai/orca");
        let r = p("https://gitlab.com/group/sub/project/-/releases");
        assert_eq!((r.forge, r.project.as_str()), (ForgeKind::Gitlab, "group/sub/project"));
        let r = p("https://codeberg.org/owner/app.git");
        assert_eq!((r.forge, r.project.as_str()), (ForgeKind::Forgejo, "owner/app"));
    }

    #[test]
    fn prefixes_and_explicit_hosts() {
        let r = p("codeberg:owner/app");
        assert_eq!((r.forge, r.host.as_str()), (ForgeKind::Forgejo, "codeberg.org"));
        let r = p("forgejo:git.example.org/owner/app");
        assert_eq!((r.host.as_str(), r.project.as_str()), ("git.example.org", "owner/app"));
        let r = p("gitlab:gitlab.example.com/a/b/c");
        assert_eq!((r.host.as_str(), r.project.as_str()), ("gitlab.example.com", "a/b/c"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("", None).is_err());
        assert!(parse("orca", None).is_err());
        assert!(parse("https://git.example.org/a/b", None).is_err());
        assert!(parse("https://git.example.org/a/b", Some(ForgeKind::Forgejo)).is_ok());
        assert!(parse("github:../etc", None).is_err());
    }
}
