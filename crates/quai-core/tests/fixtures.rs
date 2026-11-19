//! Checks the RPM reader and repodata writer against real packages, `rpm` and
//! `createrepo_c` (comparisons are skipped when those tools are absent).

use quai_core::repodata::{self, RepoPackage};
use quai_core::rpm::{self, FileKind};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rpms");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixtures missing: run tests/fixtures/build-fixtures.sh")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|e| e == "rpm").unwrap_or(false))
        .collect();
    v.sort();
    v
}

fn fixture(name: &str) -> PathBuf {
    fixtures().into_iter().find(|p| p.file_name().unwrap().to_string_lossy().starts_with(name)).unwrap()
}

fn have(tool: &str) -> bool {
    Command::new("sh").arg("-c").arg(format!("command -v {tool}")).output().map(|o| o.status.success()).unwrap_or(false)
}

fn rpm_q(path: &Path, args: &[&str]) -> Vec<String> {
    let out = Command::new("rpm").arg("-qp").args(args).arg(path).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().map(|l| l.to_string()).collect()
}

#[test]
fn headers_match_rpm() {
    for path in fixtures() {
        let bytes = std::fs::read(&path).unwrap();
        let (info, layout) = rpm::parse_file_bytes(&bytes).unwrap();
        assert!(layout.header_end > layout.header_start);
        assert!(!info.is_source);
        if !have("rpm") {
            continue;
        }
        let q = rpm_q(&path, &["--qf", "%{NAME}|%{EPOCHNUM}|%{VERSION}|%{RELEASE}|%{ARCH}|%{SUMMARY}|%{LONGSIZE}"]);
        let expected = q[0].clone();
        let got = format!(
            "{}|{}|{}|{}|{}|{}|{}",
            info.name, info.epoch, info.version, info.release, info.arch, info.summary, info.installed_size
        );
        assert_eq!(got, expected, "{}", path.display());

        let files: BTreeSet<String> = rpm_q(&path, &["--qf", "[%{FILENAMES}\n]"]).into_iter().collect();
        let ours: BTreeSet<String> = info.files.iter().map(|f| f.path.clone()).collect();
        assert_eq!(ours, files, "file list of {}", path.display());

        let provides: BTreeSet<String> = rpm_q(&path, &["--qf", "[%{PROVIDENAME}\n]"]).into_iter().collect();
        let ours: BTreeSet<String> = info.provides.iter().map(|d| d.name.clone()).collect();
        assert_eq!(ours, provides, "provides of {}", path.display());

        let requires: BTreeSet<String> = rpm_q(&path, &["--qf", "[%{REQUIRENAME}\n]"]).into_iter().collect();
        let ours: BTreeSet<String> = info.requires.iter().map(|d| d.name.clone()).collect();
        assert_eq!(ours, requires, "requires of {}", path.display());
    }
}

#[test]
fn hello_details() {
    let bytes = std::fs::read(fixture("hello-1.2.3")).unwrap();
    let (info, _) = rpm::parse_file_bytes(&bytes).unwrap();
    assert_eq!(info.evr(), "1.2.3-1");
    assert!(info.summary.contains("<loudly> & proudly"));
    assert!(info.obsoletes.iter().any(|d| d.name == "hello-old" && d.op() == Some("LT")));
    assert!(info.requires.iter().any(|d| d.name == "bash" && d.op() == Some("GE") && d.version == "4.0"));
    assert!(info.requires.iter().any(|d| d.name == "coreutils" && d.is_pre()));
    assert!(info.recommends.iter().any(|d| d.name == "cowsay"));
    assert_eq!(info.changelogs.len(), 2);
    assert!(info.changelogs[0].text.contains("Newest"));
    let kind = |p: &str| info.files.iter().find(|f| f.path == p).map(|f| f.kind);
    assert_eq!(kind("/usr/share/hello"), Some(FileKind::Dir));
    assert_eq!(kind("/var/log/hello.log"), Some(FileKind::Ghost));
    assert_eq!(kind("/usr/bin/hello"), Some(FileKind::File));
    assert!(!info.signed);

    let bytes = std::fs::read(fixture("hello-epoch")).unwrap();
    let (info, _) = rpm::parse_file_bytes(&bytes).unwrap();
    assert_eq!(info.evr(), "2:1.0-3.fc43");
    assert_eq!(info.arch, "x86_64");
}

#[test]
fn big_header_exceeds_first_chunk() {
    let bytes = std::fs::read(fixture("bigheader")).unwrap();
    let (info, layout) = rpm::parse_file_bytes(&bytes).unwrap();
    assert!(layout.header_end > 64 * 1024, "header only {} bytes", layout.header_end);
    assert_eq!(info.files.len(), 601);
}

/// Our header-range must match createrepo_c's, and dnf-relevant fields must agree.
#[test]
fn repodata_agrees_with_createrepo_c() {
    if !have("createrepo_c") {
        eprintln!("createrepo_c not installed, skipping");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    for p in fixtures() {
        std::fs::copy(&p, tmp.path().join(p.file_name().unwrap())).unwrap();
    }
    let st = Command::new("createrepo_c").arg("--quiet").arg(tmp.path()).status().unwrap();
    assert!(st.success());
    let repomd = std::fs::read_to_string(tmp.path().join("repodata/repomd.xml")).unwrap();
    let primary_href = repomd
        .split("<location href=\"")
        .skip(1)
        .map(|s| s.split('"').next().unwrap())
        .find(|h| h.contains("primary.xml"))
        .unwrap()
        .to_string();
    let gz = std::fs::read(tmp.path().join(&primary_href)).unwrap();
    let mut xml = String::new();
    if primary_href.ends_with(".gz") {
        flate2::read::GzDecoder::new(&gz[..]).read_to_string(&mut xml).unwrap();
    } else {
        eprintln!("createrepo_c used a compression other than gzip, skipping");
        return;
    }

    let mut ours = Vec::new();
    for p in fixtures() {
        let bytes = std::fs::read(&p).unwrap();
        let (info, layout) = rpm::parse_file_bytes(&bytes).unwrap();
        let name = info.name.clone();
        let ranges = format!(r#"<rpm:header-range start="{}" end="{}"/>"#, layout.header_start, layout.header_end);
        let block = xml.split("<package ").find(|b| b.contains(&format!("<name>{name}</name>"))).unwrap();
        assert!(block.contains(&ranges), "{name}: expected {ranges} in createrepo_c output");
        ours.push(RepoPackage {
            sha256: "0".repeat(64),
            package_size: bytes.len() as u64,
            file_time: 0,
            location: format!("packages/x/{}", p.file_name().unwrap().to_string_lossy()),
            header_start: layout.header_start,
            header_end: layout.header_end,
            info,
        });
    }
    let primary = repodata::primary_xml(&ours);
    for want in [
        r#"<rpm:entry name="bash" flags="GE" epoch="0" ver="4.0"/>"#,
        r#"<rpm:entry name="hello-old" flags="LT" epoch="0" ver="1.0"/>"#,
        r#"<rpm:entry name="coreutils" pre="1"/>"#,
        r#"<version epoch="2" ver="1.0" rel="3.fc43"/>"#,
        r#"<file>/etc/hello.conf</file>"#,
    ] {
        assert!(primary.contains(want), "missing {want}");
        assert!(xml.contains(want), "createrepo_c also writes {want}");
    }
}

/// Minimal HTTP server: serves one file, honouring Range unless told not to.
async fn serve(data: Vec<u8>, honour_range: bool) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let data = data.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let range = req
                    .lines()
                    .find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(|s| s.to_string()));
                let (status, body, extra) = match (range, honour_range) {
                    (Some(r), true) => {
                        let (a, b) = r.trim().split_once('-').unwrap();
                        let a: usize = a.parse().unwrap();
                        let b: usize = b.parse::<usize>().unwrap().min(data.len() - 1);
                        (
                            "206 Partial Content",
                            data[a..=b].to_vec(),
                            format!("Content-Range: bytes {a}-{b}/{}\r\n", data.len()),
                        )
                    }
                    _ => ("200 OK", data.clone(), String::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&body).await;
            });
        }
    });
    format!("http://{addr}/pkg.rpm")
}

#[tokio::test]
async fn fetch_header_with_and_without_range_support() {
    let bytes = std::fs::read(fixture("bigheader")).unwrap();
    let (_, layout) = rpm::parse_file_bytes(&bytes).unwrap();
    let http = quai_core::http::client().unwrap();
    for honour in [true, false] {
        let url = serve(bytes.clone(), honour).await;
        let got = rpm::fetch_header(&http, &url).await.unwrap();
        assert_eq!(got.layout, layout);
        assert_eq!(got.bytes, bytes[..layout.header_end].to_vec());
        assert_eq!(got.total_size, Some(bytes.len() as u64));
    }
    let url = serve(bytes.clone(), true).await;
    let (sum, size) = rpm::sha256_of_url(&http, &url).await.unwrap();
    assert_eq!(size, bytes.len() as u64);
    use sha2::Digest;
    assert_eq!(sum, hex::encode(sha2::Sha256::digest(&bytes)));
}
