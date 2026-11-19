//! Distribution tags (`.fc45`, `.el10`): which build of a package fits this system.
//!
//! Projects that support several Fedora releases often attach one build per
//! release (`foo-1.0-1.fc44.x86_64.rpm`, `foo-1.0-1.fc45.x86_64.rpm`). quai
//! serves the build made for the running system instead of asking the user to
//! choose, and switches by itself after a system upgrade.

use quai_i18n::trf;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    /// Fedora and its derivatives: `fcNN`.
    Fedora,
    /// RHEL, CentOS Stream, AlmaLinux, Rocky Linux...: `elNN`.
    Enterprise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dist {
    pub family: Family,
    pub version: u32,
}

impl Dist {
    pub fn fedora(version: u32) -> Self {
        Dist { family: Family::Fedora, version }
    }

    /// The tag as it appears in a release: `fc45`, `el10`.
    pub fn tag(&self) -> String {
        match self.family {
            Family::Fedora => format!("fc{}", self.version),
            Family::Enterprise => format!("el{}", self.version),
        }
    }

    /// Parses `fc45`, `el10`, `el10_1` (with or without a leading dot).
    pub fn parse_tag(s: &str) -> Option<Dist> {
        let s = s.trim().trim_start_matches('.');
        let (family, rest) = match s.strip_prefix("fc") {
            Some(r) => (Family::Fedora, r),
            None => (Family::Enterprise, s.strip_prefix("el")?),
        };
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let tail = &rest[digits.len()..];
        if digits.is_empty() || !(tail.is_empty() || tail.starts_with('_')) {
            return None;
        }
        Some(Dist { family, version: digits.parse().ok()? })
    }

    /// The dist tag of an RPM release such as `1.fc45` or `3.el10_1`; the last one wins.
    pub fn from_release(release: &str) -> Option<Dist> {
        release.split('.').rev().find_map(Dist::parse_tag)
    }

    /// Human name: "Fedora 45", "Enterprise Linux 10".
    pub fn display(&self) -> String {
        match self.family {
            Family::Fedora => trf("Fedora {version}", &[("version", &self.version.to_string())]),
            Family::Enterprise => trf("Enterprise Linux {version}", &[("version", &self.version.to_string())]),
        }
    }
}

impl fmt::Display for Dist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.tag())
    }
}

/// The distribution this system runs, from `QUAI_DIST` (tests; empty disables
/// the matching) or os-release.
pub fn system_dist() -> Option<Dist> {
    if let Some(v) = std::env::var_os("QUAI_DIST") {
        return Dist::parse_tag(&v.to_string_lossy());
    }
    ["/etc/os-release", "/usr/lib/os-release"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| from_os_release(&text))
}

/// Reads ID, ID_LIKE and VERSION_ID. Derivatives that keep Fedora's release
/// numbers (Ultramarine, Nobara...) count as Fedora.
pub fn from_os_release(text: &str) -> Option<Dist> {
    let mut id = String::new();
    let mut like = String::new();
    let mut version = String::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim().trim_matches('"').trim_matches('\'').to_ascii_lowercase();
        match k.trim() {
            "ID" => id = v,
            "ID_LIKE" => like = v,
            "VERSION_ID" => version = v,
            _ => {}
        }
    }
    let major: u32 = version.split('.').next()?.parse().ok()?;
    let like: Vec<&str> = like.split_whitespace().collect();
    let enterprise = ["rhel", "centos", "almalinux", "rocky", "ol", "circle", "eurolinux"];
    if id == "fedora" || (like.contains(&"fedora") && major >= 30) {
        Some(Dist { family: Family::Fedora, version: major })
    } else if enterprise.contains(&id.as_str()) || like.iter().any(|l| enterprise.contains(l)) {
        Some(Dist { family: Family::Enterprise, version: major })
    } else {
        None
    }
}

/// How a build was chosen for this system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Made for this release.
    Exact,
    /// Made for an older release of the same distribution.
    Older(Dist),
    /// No dist tag.
    Untagged,
    /// Only builds for newer releases exist.
    Newer(Dist),
}

/// Among the builds of one package for one architecture in one release, the
/// ones to serve on `system`: the build for this release, else the newest
/// older one, else an untagged one, else the oldest newer one.
///
/// Returns every index (and no fit) when there is nothing to choose: a single
/// build, an unknown system, or builds that do not differ by dist tag. Those
/// remain variants for the user to pick with an asset pattern.
pub fn pick(dists: &[Option<Dist>], system: Option<Dist>) -> (Vec<usize>, Option<Fit>) {
    let all = || (0..dists.len()).collect::<Vec<_>>();
    let Some(sys) = system else { return (all(), None) };
    let distinct: std::collections::HashSet<Option<Dist>> = dists.iter().copied().collect();
    if dists.len() < 2 || distinct.len() < 2 {
        return (all(), None);
    }
    let same = |d: &Option<Dist>| d.filter(|d| d.family == sys.family);
    let select = |want: Option<Dist>| -> Vec<usize> { (0..dists.len()).filter(|&i| dists[i] == want).collect() };

    if dists.contains(&Some(sys)) {
        return (select(Some(sys)), Some(Fit::Exact));
    }
    if let Some(older) = dists.iter().filter_map(same).filter(|d| d.version < sys.version).max_by_key(|d| d.version) {
        return (select(Some(older)), Some(Fit::Older(older)));
    }
    if dists.contains(&None) {
        return (select(None), Some(Fit::Untagged));
    }
    if let Some(newer) = dists.iter().filter_map(same).filter(|d| d.version > sys.version).min_by_key(|d| d.version) {
        return (select(Some(newer)), Some(Fit::Newer(newer)));
    }
    (all(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(tag: &str) -> Option<Dist> {
        Dist::parse_tag(tag)
    }

    #[test]
    fn tags_and_releases() {
        assert_eq!(Dist::parse_tag("fc45"), Some(Dist::fedora(45)));
        assert_eq!(Dist::parse_tag(".el10_1"), Some(Dist { family: Family::Enterprise, version: 10 }));
        assert_eq!(Dist::parse_tag("fcx"), None);
        assert_eq!(Dist::parse_tag("el9x"), None);
        assert_eq!(Dist::parse_tag("elephant"), None);
        assert_eq!(Dist::from_release("1.fc45"), Some(Dist::fedora(45)));
        assert_eq!(Dist::from_release("0.3.beta.fc44.1"), Some(Dist::fedora(44)));
        assert_eq!(Dist::from_release("1"), None);
        assert_eq!(Dist::fedora(45).tag(), "fc45");
    }

    #[test]
    fn os_release() {
        let fedora = "NAME=\"Fedora Linux\"\nID=fedora\nVERSION_ID=45\n";
        assert_eq!(from_os_release(fedora), Some(Dist::fedora(45)));
        let ultramarine = "ID=ultramarine\nID_LIKE=fedora\nVERSION_ID=44\n";
        assert_eq!(from_os_release(ultramarine), Some(Dist::fedora(44)));
        let alma = "ID=\"almalinux\"\nID_LIKE=\"rhel centos fedora\"\nVERSION_ID=\"10.1\"\n";
        assert_eq!(from_os_release(alma), Some(Dist { family: Family::Enterprise, version: 10 }));
        assert_eq!(from_os_release("ID=ubuntu\nVERSION_ID=\"24.04\"\n"), None);
    }

    #[test]
    fn picks_the_build_for_this_system() {
        let builds = [d("fc44"), d("fc45"), d("el10")];
        assert_eq!(pick(&builds, d("fc45")), (vec![1], Some(Fit::Exact)));
        assert_eq!(pick(&builds, d("fc46")), (vec![1], Some(Fit::Older(Dist::fedora(45)))));
        assert_eq!(pick(&builds, d("fc43")), (vec![0], Some(Fit::Newer(Dist::fedora(44)))));
        assert_eq!(pick(&builds, d("el10")), (vec![2], Some(Fit::Exact)));
        assert_eq!(pick(&[d("fc44"), None], d("fc43")), (vec![1], Some(Fit::Untagged)));
    }

    #[test]
    fn nothing_to_choose_keeps_every_build() {
        assert_eq!(pick(&[d("fc45"), d("fc45")], d("fc45")), (vec![0, 1], None));
        assert_eq!(pick(&[d("fc44"), d("fc45")], None), (vec![0, 1], None));
        assert_eq!(pick(&[d("fc45")], d("fc44")), (vec![0], None));
        assert_eq!(pick(&[d("el9"), d("el10")], d("fc45")), (vec![0, 1], None));
    }
}
