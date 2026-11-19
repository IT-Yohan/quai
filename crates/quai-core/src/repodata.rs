//! Generate rpm-md repository metadata (the format createrepo_c writes).
//!
//! Output is deterministic: the same packages and revision always produce the
//! same bytes, so dnf never re-downloads metadata that did not change.

use crate::rpm::{Dep, FileKind, PackageInfo};
use flate2::{Compression, GzBuilder};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Write;

/// Number of changelog entries kept in `other.xml` (createrepo_c's default).
pub const CHANGELOG_LIMIT: usize = 10;

/// A package as it will appear in the repository.
#[derive(Clone, Debug)]
pub struct RepoPackage {
    pub info: PackageInfo,
    pub sha256: String,
    pub package_size: u64,
    /// Unix time used as the file time (upload time of the asset).
    pub file_time: i64,
    /// Path relative to the repository root, e.g. `packages/<key>/<file>.rpm`.
    pub location: String,
    pub header_start: usize,
    pub header_end: usize,
}

pub struct RepodataFile {
    /// File name inside `repodata/`, e.g. `<sha256>-primary.xml.gz`.
    pub name: String,
    pub bytes: Vec<u8>,
}

pub struct Repodata {
    pub repomd: Vec<u8>,
    pub files: Vec<RepodataFile>,
}

/// Escape text for XML 1.0, dropping characters XML cannot represent.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => {}
            '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// Files listed in primary.xml as well as filelists.xml (createrepo's rule).
fn is_primary_file(path: &str) -> bool {
    path.starts_with("/etc/") || path == "/usr/lib/sendmail" || path.contains("bin/")
}

fn version_attrs(epoch: u32, version: &str, release: &str) -> String {
    format!(r#"epoch="{}" ver="{}" rel="{}""#, epoch, esc(version), esc(release))
}

fn dep_entry(out: &mut String, d: &Dep, with_pre: bool) {
    let _ = write!(out, r#"      <rpm:entry name="{}""#, esc(&d.name));
    if let Some(op) = d.op() {
        let (e, v, r) = crate::evr::parse_evr(&d.version);
        let _ = write!(out, r#" flags="{op}" epoch="{e}" ver="{}""#, esc(&v));
        if !r.is_empty() {
            let _ = write!(out, r#" rel="{}""#, esc(&r));
        }
    }
    if with_pre && d.is_pre() {
        out.push_str(r#" pre="1""#);
    }
    out.push_str("/>\n");
}

fn dep_block(out: &mut String, tag: &str, deps: &[Dep], with_pre: bool) {
    if deps.is_empty() {
        return;
    }
    let _ = writeln!(out, "    <rpm:{tag}>");
    for d in deps {
        dep_entry(out, d, with_pre);
    }
    let _ = writeln!(out, "    </rpm:{tag}>");
}

/// Requires as createrepo_c lists them: no `rpmlib(...)`, no duplicates, and no
/// files the package ships itself among its primary files.
fn filtered_requires(info: &PackageInfo) -> Vec<Dep> {
    let own_primary: HashSet<&str> =
        info.files.iter().map(|f| f.path.as_str()).filter(|p| is_primary_file(p)).collect();
    let mut seen = HashSet::new();
    info.requires
        .iter()
        .filter(|d| !d.name.starts_with("rpmlib("))
        .filter(|d| !(d.name.starts_with('/') && own_primary.contains(d.name.as_str())))
        .filter(|d| seen.insert((d.name.clone(), d.flags & 0x0e, d.version.clone())))
        .cloned()
        .collect()
}

fn dedup(deps: &[Dep]) -> Vec<Dep> {
    let mut seen = HashSet::new();
    deps.iter().filter(|d| seen.insert((d.name.clone(), d.flags & 0x0e, d.version.clone()))).cloned().collect()
}

fn file_line(out: &mut String, indent: &str, path: &str, kind: FileKind) {
    match kind {
        FileKind::File => {
            let _ = writeln!(out, "{indent}<file>{}</file>", esc(path));
        }
        FileKind::Dir => {
            let _ = writeln!(out, r#"{indent}<file type="dir">{}</file>"#, esc(path));
        }
        FileKind::Ghost => {
            let _ = writeln!(out, r#"{indent}<file type="ghost">{}</file>"#, esc(path));
        }
    }
}

pub fn primary_xml(packages: &[RepoPackage]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        r#"<metadata xmlns="http://linux.duke.edu/metadata/common" xmlns:rpm="http://linux.duke.edu/metadata/rpm" packages="{}">"#,
        packages.len()
    );
    for p in packages {
        let i = &p.info;
        out.push_str("<package type=\"rpm\">\n");
        let _ = writeln!(out, "  <name>{}</name>", esc(&i.name));
        let _ = writeln!(out, "  <arch>{}</arch>", esc(&i.arch));
        let _ = writeln!(out, "  <version {}/>", version_attrs(i.epoch, &i.version, &i.release));
        let _ = writeln!(out, r#"  <checksum type="sha256" pkgid="YES">{}</checksum>"#, p.sha256);
        let _ = writeln!(out, "  <summary>{}</summary>", esc(&i.summary));
        let _ = writeln!(out, "  <description>{}</description>", esc(&i.description));
        let _ = writeln!(out, "  <packager>{}</packager>", esc(&i.packager));
        let _ = writeln!(out, "  <url>{}</url>", esc(&i.url));
        let _ = writeln!(out, r#"  <time file="{}" build="{}"/>"#, p.file_time, i.build_time);
        let _ = writeln!(
            out,
            r#"  <size package="{}" installed="{}" archive="{}"/>"#,
            p.package_size, i.installed_size, i.archive_size
        );
        let _ = writeln!(out, r#"  <location href="{}"/>"#, esc(&p.location));
        out.push_str("  <format>\n");
        let _ = writeln!(out, "    <rpm:license>{}</rpm:license>", esc(&i.license));
        let _ = writeln!(out, "    <rpm:vendor>{}</rpm:vendor>", esc(&i.vendor));
        let _ = writeln!(out, "    <rpm:group>{}</rpm:group>", esc(&i.group));
        let _ = writeln!(out, "    <rpm:buildhost>{}</rpm:buildhost>", esc(&i.buildhost));
        let _ = writeln!(out, "    <rpm:sourcerpm>{}</rpm:sourcerpm>", esc(&i.sourcerpm));
        let _ = writeln!(out, r#"    <rpm:header-range start="{}" end="{}"/>"#, p.header_start, p.header_end);
        dep_block(&mut out, "provides", &dedup(&i.provides), false);
        dep_block(&mut out, "requires", &filtered_requires(i), true);
        dep_block(&mut out, "conflicts", &dedup(&i.conflicts), false);
        dep_block(&mut out, "obsoletes", &dedup(&i.obsoletes), false);
        dep_block(&mut out, "suggests", &dedup(&i.suggests), false);
        dep_block(&mut out, "enhances", &dedup(&i.enhances), false);
        dep_block(&mut out, "recommends", &dedup(&i.recommends), false);
        dep_block(&mut out, "supplements", &dedup(&i.supplements), false);
        for f in i.files.iter().filter(|f| is_primary_file(&f.path)) {
            file_line(&mut out, "    ", &f.path, f.kind);
        }
        out.push_str("  </format>\n");
        out.push_str("</package>\n");
    }
    out.push_str("</metadata>\n");
    out
}

pub fn filelists_xml(packages: &[RepoPackage]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        r#"<filelists xmlns="http://linux.duke.edu/metadata/filelists" packages="{}">"#,
        packages.len()
    );
    for p in packages {
        let i = &p.info;
        let _ = writeln!(
            out,
            r#"<package pkgid="{}" name="{}" arch="{}">"#,
            p.sha256,
            esc(&i.name),
            esc(&i.arch)
        );
        let _ = writeln!(out, "  <version {}/>", version_attrs(i.epoch, &i.version, &i.release));
        for f in &i.files {
            file_line(&mut out, "  ", &f.path, f.kind);
        }
        out.push_str("</package>\n");
    }
    out.push_str("</filelists>\n");
    out
}

pub fn other_xml(packages: &[RepoPackage]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        r#"<otherdata xmlns="http://linux.duke.edu/metadata/other" packages="{}">"#,
        packages.len()
    );
    for p in packages {
        let i = &p.info;
        let _ = writeln!(
            out,
            r#"<package pkgid="{}" name="{}" arch="{}">"#,
            p.sha256,
            esc(&i.name),
            esc(&i.arch)
        );
        let _ = writeln!(out, "  <version {}/>", version_attrs(i.epoch, &i.version, &i.release));
        // Header order is newest first; keep the newest entries, written oldest first.
        let mut entries: Vec<_> = i.changelogs.iter().take(CHANGELOG_LIMIT).collect();
        entries.reverse();
        for c in entries {
            let _ = writeln!(
                out,
                r#"  <changelog author="{}" date="{}">{}</changelog>"#,
                esc(&c.author),
                c.time,
                esc(&c.text)
            );
        }
        out.push_str("</package>\n");
    }
    out.push_str("</otherdata>\n");
    out
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut enc = GzBuilder::new().mtime(0).write(Vec::new(), Compression::new(6));
    enc.write_all(data).expect("writing to a Vec cannot fail");
    enc.finish().expect("writing to a Vec cannot fail")
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Build all metadata for `packages`. `revision` doubles as the timestamp.
pub fn build(packages: &[RepoPackage], revision: i64) -> Repodata {
    let parts = [
        ("primary", primary_xml(packages)),
        ("filelists", filelists_xml(packages)),
        ("other", other_xml(packages)),
    ];
    let mut files = Vec::new();
    let mut repomd = String::new();
    repomd.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    repomd.push_str(
        "<repomd xmlns=\"http://linux.duke.edu/metadata/repo\" xmlns:rpm=\"http://linux.duke.edu/metadata/rpm\">\n",
    );
    let _ = writeln!(repomd, "  <revision>{revision}</revision>");
    for (kind, xml) in parts {
        let open = xml.into_bytes();
        let gz = gzip(&open);
        let gz_sum = sha256_hex(&gz);
        let name = format!("{gz_sum}-{kind}.xml.gz");
        let _ = writeln!(repomd, "  <data type=\"{kind}\">");
        let _ = writeln!(repomd, "    <checksum type=\"sha256\">{gz_sum}</checksum>");
        let _ = writeln!(repomd, "    <open-checksum type=\"sha256\">{}</open-checksum>", sha256_hex(&open));
        let _ = writeln!(repomd, "    <location href=\"repodata/{name}\"/>");
        let _ = writeln!(repomd, "    <timestamp>{revision}</timestamp>");
        let _ = writeln!(repomd, "    <size>{}</size>", gz.len());
        let _ = writeln!(repomd, "    <open-size>{}</open-size>", open.len());
        let _ = writeln!(repomd, "  </data>");
        files.push(RepodataFile { name, bytes: gz });
    }
    repomd.push_str("</repomd>\n");
    Repodata { repomd: repomd.into_bytes(), files }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_handles_markup_and_control_chars() {
        assert_eq!(esc("a<b>&\"c\u{1}"), "a&lt;b&gt;&amp;&quot;c");
    }

    #[test]
    fn empty_repo_is_deterministic() {
        let a = build(&[], 42);
        let b = build(&[], 42);
        assert_eq!(a.repomd, b.repomd);
        assert_eq!(a.files.len(), 3);
        let text = String::from_utf8(a.repomd).unwrap();
        assert!(text.contains("<revision>42</revision>"));
        assert!(text.contains("-primary.xml.gz\"/>"));
    }
}
