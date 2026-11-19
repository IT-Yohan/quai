//! Minimal reader for the RPM v3/v4 file format: lead, signature header, main header.
//!
//! Only the header is ever read; the payload never is. Layout:
//!
//! ```text
//! lead (96 B) | signature header, padded to 8 B | main header | payload
//!             ^ 96                               ^ header_start ^ header_end
//! ```
//!
//! Each header is `8e ad e8 01 00000000 <nindex:u32> <hsize:u32>` followed by
//! `nindex` 16-byte entries (`tag, type, offset, count`, big endian) and `hsize`
//! bytes of data.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;

pub const LEAD_SIZE: usize = 96;
const LEAD_MAGIC: [u8; 4] = [0xed, 0xab, 0xee, 0xdb];
const HEADER_MAGIC: [u8; 3] = [0x8e, 0xad, 0xe8];
/// Refuse absurd headers (a broken or hostile file) instead of allocating them.
const MAX_HEADER_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;

// Data types.
const T_CHAR: u32 = 1;
const T_INT8: u32 = 2;
const T_INT16: u32 = 3;
const T_INT32: u32 = 4;
const T_INT64: u32 = 5;
const T_STRING: u32 = 6;
const T_BIN: u32 = 7;
const T_STRING_ARRAY: u32 = 8;
const T_I18NSTRING: u32 = 9;

// Main header tags.
pub mod tag {
    pub const NAME: u32 = 1000;
    pub const VERSION: u32 = 1001;
    pub const RELEASE: u32 = 1002;
    pub const EPOCH: u32 = 1003;
    pub const SUMMARY: u32 = 1004;
    pub const DESCRIPTION: u32 = 1005;
    pub const BUILDTIME: u32 = 1006;
    pub const BUILDHOST: u32 = 1007;
    pub const SIZE: u32 = 1009;
    pub const VENDOR: u32 = 1011;
    pub const LICENSE: u32 = 1014;
    pub const PACKAGER: u32 = 1015;
    pub const GROUP: u32 = 1016;
    pub const URL: u32 = 1020;
    pub const ARCH: u32 = 1022;
    pub const OLDFILENAMES: u32 = 1027;
    pub const FILEMODES: u32 = 1030;
    pub const FILEFLAGS: u32 = 1037;
    pub const SOURCERPM: u32 = 1044;
    pub const ARCHIVESIZE: u32 = 1046;
    pub const PROVIDENAME: u32 = 1047;
    pub const REQUIREFLAGS: u32 = 1048;
    pub const REQUIRENAME: u32 = 1049;
    pub const REQUIREVERSION: u32 = 1050;
    pub const CONFLICTFLAGS: u32 = 1053;
    pub const CONFLICTNAME: u32 = 1054;
    pub const CONFLICTVERSION: u32 = 1055;
    pub const CHANGELOGTIME: u32 = 1080;
    pub const CHANGELOGNAME: u32 = 1081;
    pub const CHANGELOGTEXT: u32 = 1082;
    pub const OBSOLETENAME: u32 = 1090;
    pub const PROVIDEFLAGS: u32 = 1112;
    pub const PROVIDEVERSION: u32 = 1113;
    pub const OBSOLETEFLAGS: u32 = 1114;
    pub const OBSOLETEVERSION: u32 = 1115;
    pub const DIRINDEXES: u32 = 1116;
    pub const BASENAMES: u32 = 1117;
    pub const DIRNAMES: u32 = 1118;
    pub const LONGARCHIVESIZE: u32 = 271;
    pub const LONGSIZE: u32 = 5009;
    pub const RECOMMENDNAME: u32 = 5046;
    pub const RECOMMENDVERSION: u32 = 5047;
    pub const RECOMMENDFLAGS: u32 = 5048;
    pub const SUGGESTNAME: u32 = 5049;
    pub const SUGGESTVERSION: u32 = 5050;
    pub const SUGGESTFLAGS: u32 = 5051;
    pub const SUPPLEMENTNAME: u32 = 5052;
    pub const SUPPLEMENTVERSION: u32 = 5053;
    pub const SUPPLEMENTFLAGS: u32 = 5054;
    pub const ENHANCENAME: u32 = 5055;
    pub const ENHANCEVERSION: u32 = 5056;
    pub const ENHANCEFLAGS: u32 = 5057;

    // Signature header tags.
    pub const SIG_PAYLOADSIZE: u32 = 1007;
    pub const SIG_LONGARCHIVESIZE: u32 = 271;
    pub const SIG_DSA: u32 = 267;
    pub const SIG_RSA: u32 = 268;
    pub const SIG_OPENPGP: u32 = 278;
    pub const SIG_PGP: u32 = 1002;
    pub const SIG_GPG: u32 = 1005;
}

/// Byte offsets of the parts of an RPM file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Layout {
    pub sig_len: usize,
    pub header_start: usize,
    pub header_end: usize,
}

pub enum Probe {
    /// At least this many bytes from the start of the file are needed.
    NeedBytes(usize),
    Layout(Layout),
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Read a header intro and return (nindex, hsize).
fn intro(b: &[u8]) -> Result<(usize, usize)> {
    if b.len() < 16 {
        bail!("truncated RPM header");
    }
    if b[0..3] != HEADER_MAGIC || b[3] != 1 {
        bail!("bad RPM header magic");
    }
    let nindex = be32(b, 8) as usize;
    let hsize = be32(b, 12) as usize;
    if nindex > MAX_ENTRIES || hsize > MAX_HEADER_BYTES {
        bail!("RPM header too large ({nindex} entries, {hsize} bytes)");
    }
    Ok((nindex, hsize))
}

/// Work out where the main header ends from the first bytes of a file.
pub fn probe(buf: &[u8]) -> Result<Probe> {
    if buf.len() < LEAD_SIZE + 16 {
        return Ok(Probe::NeedBytes(LEAD_SIZE + 16));
    }
    if buf[0..4] != LEAD_MAGIC {
        bail!("not an RPM file (bad lead magic)");
    }
    let (n, d) = intro(&buf[LEAD_SIZE..])?;
    let sig_len = 16 + 16 * n + d;
    let pad = (8 - sig_len % 8) % 8;
    let header_start = LEAD_SIZE + sig_len + pad;
    if buf.len() < header_start + 16 {
        return Ok(Probe::NeedBytes(header_start + 16));
    }
    let (n2, d2) = intro(&buf[header_start..])?;
    let header_end = header_start + 16 + 16 * n2 + d2;
    if header_end > MAX_HEADER_BYTES {
        bail!("RPM header too large");
    }
    Ok(Probe::Layout(Layout { sig_len, header_start, header_end }))
}

/// Lead says whether this is a source package (type 1).
pub fn lead_is_source(buf: &[u8]) -> bool {
    buf.len() >= 8 && u16::from_be_bytes([buf[6], buf[7]]) == 1
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    ty: u32,
    offset: usize,
    count: usize,
}

/// A parsed header: index entries pointing into the data store.
pub struct Header<'a> {
    entries: HashMap<u32, Entry>,
    data: &'a [u8],
}

impl<'a> Header<'a> {
    /// `bytes` starts at the header magic.
    pub fn parse(bytes: &'a [u8]) -> Result<Header<'a>> {
        let (n, d) = intro(bytes)?;
        let index_end = 16 + 16 * n;
        if bytes.len() < index_end + d {
            bail!("truncated RPM header ({} of {} bytes)", bytes.len(), index_end + d);
        }
        let data = &bytes[index_end..index_end + d];
        let mut entries = HashMap::with_capacity(n);
        for i in 0..n {
            let at = 16 + 16 * i;
            let tag = be32(bytes, at);
            let ty = be32(bytes, at + 4);
            let offset = be32(bytes, at + 8) as usize;
            let count = be32(bytes, at + 12) as usize;
            if offset > d {
                bail!("RPM header entry {tag} points outside the data store");
            }
            entries.entry(tag).or_insert(Entry { ty, offset, count });
        }
        Ok(Header { entries, data })
    }

    pub fn has(&self, tag: u32) -> bool {
        self.entries.contains_key(&tag)
    }

    fn cstr_at(&self, at: usize) -> Option<(String, usize)> {
        let rest = self.data.get(at..)?;
        let len = rest.iter().position(|&b| b == 0)?;
        Some((String::from_utf8_lossy(&rest[..len]).into_owned(), at + len + 1))
    }

    /// First string of a STRING, I18NSTRING or STRING_ARRAY entry.
    pub fn string(&self, tag: u32) -> Option<String> {
        let e = self.entries.get(&tag)?;
        match e.ty {
            T_STRING | T_I18NSTRING | T_STRING_ARRAY => self.cstr_at(e.offset).map(|(s, _)| s),
            _ => None,
        }
    }

    pub fn string_array(&self, tag: u32) -> Vec<String> {
        let Some(e) = self.entries.get(&tag) else {
            return Vec::new();
        };
        if !matches!(e.ty, T_STRING | T_STRING_ARRAY | T_I18NSTRING) {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(e.count.min(65_536));
        let mut at = e.offset;
        for _ in 0..e.count {
            match self.cstr_at(at) {
                Some((s, next)) => {
                    out.push(s);
                    at = next;
                }
                None => break,
            }
        }
        out
    }

    /// Integer array of any width, widened to u64.
    pub fn ints(&self, tag: u32) -> Vec<u64> {
        let Some(e) = self.entries.get(&tag) else {
            return Vec::new();
        };
        let width = match e.ty {
            T_CHAR | T_INT8 | T_BIN => 1,
            T_INT16 => 2,
            T_INT32 => 4,
            T_INT64 => 8,
            _ => return Vec::new(),
        };
        let Some(end) = e.count.checked_mul(width).and_then(|n| n.checked_add(e.offset)) else {
            return Vec::new();
        };
        let Some(slice) = self.data.get(e.offset..end) else {
            return Vec::new();
        };
        slice
            .chunks_exact(width)
            .map(|c| match width {
                1 => c[0] as u64,
                2 => u16::from_be_bytes([c[0], c[1]]) as u64,
                4 => u32::from_be_bytes([c[0], c[1], c[2], c[3]]) as u64,
                _ => u64::from_be_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]),
            })
            .collect()
    }

    pub fn int(&self, tag: u32) -> Option<u64> {
        self.ints(tag).first().copied()
    }
}

// Dependency flags.
pub const SENSE_LESS: u32 = 0x02;
pub const SENSE_GREATER: u32 = 0x04;
pub const SENSE_EQUAL: u32 = 0x08;
pub const SENSE_PREREQ: u32 = 0x40;
pub const SENSE_SCRIPT_PRE: u32 = 0x200;
pub const SENSE_SCRIPT_POST: u32 = 0x400;

const FILE_GHOST: u64 = 1 << 6;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Dep {
    pub name: String,
    pub flags: u32,
    pub version: String,
}

impl Dep {
    /// `LT`, `LE`, `EQ`, `GE`, `GT`, or `None` for an unversioned dependency.
    pub fn op(&self) -> Option<&'static str> {
        if self.version.is_empty() {
            return None;
        }
        let f = self.flags & (SENSE_LESS | SENSE_GREATER | SENSE_EQUAL);
        match f {
            x if x == SENSE_LESS => Some("LT"),
            x if x == SENSE_LESS | SENSE_EQUAL => Some("LE"),
            x if x == SENSE_EQUAL => Some("EQ"),
            x if x == SENSE_GREATER | SENSE_EQUAL => Some("GE"),
            x if x == SENSE_GREATER => Some("GT"),
            _ => None,
        }
    }

    pub fn is_pre(&self) -> bool {
        self.flags & (SENSE_PREREQ | SENSE_SCRIPT_PRE | SENSE_SCRIPT_POST) != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    File,
    Dir,
    Ghost,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub kind: FileKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changelog {
    pub author: String,
    pub time: u64,
    pub text: String,
}

/// Everything repodata needs from a package header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageInfo {
    pub name: String,
    pub epoch: u32,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub summary: String,
    pub description: String,
    pub packager: String,
    pub url: String,
    pub license: String,
    pub vendor: String,
    pub group: String,
    pub buildhost: String,
    pub sourcerpm: String,
    pub build_time: u64,
    pub installed_size: u64,
    pub archive_size: u64,
    pub provides: Vec<Dep>,
    pub requires: Vec<Dep>,
    pub conflicts: Vec<Dep>,
    pub obsoletes: Vec<Dep>,
    pub recommends: Vec<Dep>,
    pub suggests: Vec<Dep>,
    pub supplements: Vec<Dep>,
    pub enhances: Vec<Dep>,
    pub files: Vec<FileEntry>,
    pub changelogs: Vec<Changelog>,
    pub is_source: bool,
    /// The signature header carries an OpenPGP signature (RSA, DSA or v6).
    pub signed: bool,
}

impl PackageInfo {
    /// `epoch:version-release`, with the epoch left out when it is zero.
    pub fn evr(&self) -> String {
        crate::evr::format_evr(self.epoch, &self.version, &self.release)
    }

    pub fn nevra(&self) -> String {
        format!("{}-{}.{}", self.name, self.evr(), self.arch)
    }
}

fn deps(h: &Header, names: u32, flags: u32, versions: u32) -> Vec<Dep> {
    let n = h.string_array(names);
    let f = h.ints(flags);
    let v = h.string_array(versions);
    n.into_iter()
        .enumerate()
        .map(|(i, name)| Dep {
            name,
            flags: f.get(i).copied().unwrap_or(0) as u32,
            version: v.get(i).cloned().unwrap_or_default(),
        })
        .collect()
}

/// Parse lead + signature header + main header (`buf` holds at least `layout.header_end` bytes).
pub fn parse_package(buf: &[u8], layout: &Layout) -> Result<PackageInfo> {
    if buf.len() < layout.header_end {
        bail!("RPM header incomplete: have {} of {} bytes", buf.len(), layout.header_end);
    }
    let sig = Header::parse(&buf[LEAD_SIZE..LEAD_SIZE + layout.sig_len]).context("signature header")?;
    let h = Header::parse(&buf[layout.header_start..layout.header_end]).context("main header")?;

    let name = h.string(tag::NAME).context("package has no name")?;
    let version = h.string(tag::VERSION).context("package has no version")?;
    let release = h.string(tag::RELEASE).unwrap_or_default();
    let arch = h.string(tag::ARCH).unwrap_or_else(|| "noarch".to_string());

    // File list: compressed (basenames + dirnames) or legacy full paths.
    let modes = h.ints(tag::FILEMODES);
    let fflags = h.ints(tag::FILEFLAGS);
    let mut paths = Vec::new();
    let basenames = h.string_array(tag::BASENAMES);
    if !basenames.is_empty() {
        let dirnames = h.string_array(tag::DIRNAMES);
        let dirindexes = h.ints(tag::DIRINDEXES);
        for (i, base) in basenames.iter().enumerate() {
            let dir = dirindexes.get(i).and_then(|&d| dirnames.get(d as usize)).map(|s| s.as_str()).unwrap_or("");
            paths.push(format!("{dir}{base}"));
        }
    } else {
        paths = h.string_array(tag::OLDFILENAMES);
    }
    let files = paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| {
            let mode = modes.get(i).copied().unwrap_or(0);
            let flags = fflags.get(i).copied().unwrap_or(0);
            let kind = if flags & FILE_GHOST != 0 {
                FileKind::Ghost
            } else if mode & 0o170000 == 0o040000 {
                FileKind::Dir
            } else {
                FileKind::File
            };
            FileEntry { path, kind }
        })
        .collect();

    let times = h.ints(tag::CHANGELOGTIME);
    let authors = h.string_array(tag::CHANGELOGNAME);
    let texts = h.string_array(tag::CHANGELOGTEXT);
    let changelogs = times
        .iter()
        .enumerate()
        .map(|(i, &time)| Changelog {
            author: authors.get(i).cloned().unwrap_or_default(),
            time,
            text: texts.get(i).cloned().unwrap_or_default(),
        })
        .collect();

    let archive_size = sig
        .int(tag::SIG_LONGARCHIVESIZE)
        .or_else(|| sig.int(tag::SIG_PAYLOADSIZE))
        .or_else(|| h.int(tag::LONGARCHIVESIZE))
        .or_else(|| h.int(tag::ARCHIVESIZE))
        .unwrap_or(0);

    Ok(PackageInfo {
        epoch: h.int(tag::EPOCH).unwrap_or(0) as u32,
        summary: h.string(tag::SUMMARY).unwrap_or_default(),
        description: h.string(tag::DESCRIPTION).unwrap_or_default(),
        packager: h.string(tag::PACKAGER).unwrap_or_default(),
        url: h.string(tag::URL).unwrap_or_default(),
        license: h.string(tag::LICENSE).unwrap_or_default(),
        vendor: h.string(tag::VENDOR).unwrap_or_default(),
        group: h.string(tag::GROUP).unwrap_or_default(),
        buildhost: h.string(tag::BUILDHOST).unwrap_or_default(),
        sourcerpm: h.string(tag::SOURCERPM).unwrap_or_default(),
        build_time: h.int(tag::BUILDTIME).unwrap_or(0),
        installed_size: h.int(tag::LONGSIZE).or_else(|| h.int(tag::SIZE)).unwrap_or(0),
        archive_size,
        provides: deps(&h, tag::PROVIDENAME, tag::PROVIDEFLAGS, tag::PROVIDEVERSION),
        requires: deps(&h, tag::REQUIRENAME, tag::REQUIREFLAGS, tag::REQUIREVERSION),
        conflicts: deps(&h, tag::CONFLICTNAME, tag::CONFLICTFLAGS, tag::CONFLICTVERSION),
        obsoletes: deps(&h, tag::OBSOLETENAME, tag::OBSOLETEFLAGS, tag::OBSOLETEVERSION),
        recommends: deps(&h, tag::RECOMMENDNAME, tag::RECOMMENDFLAGS, tag::RECOMMENDVERSION),
        suggests: deps(&h, tag::SUGGESTNAME, tag::SUGGESTFLAGS, tag::SUGGESTVERSION),
        supplements: deps(&h, tag::SUPPLEMENTNAME, tag::SUPPLEMENTFLAGS, tag::SUPPLEMENTVERSION),
        enhances: deps(&h, tag::ENHANCENAME, tag::ENHANCEFLAGS, tag::ENHANCEVERSION),
        files,
        changelogs,
        is_source: lead_is_source(buf),
        signed: [tag::SIG_DSA, tag::SIG_RSA, tag::SIG_PGP, tag::SIG_GPG, tag::SIG_OPENPGP].iter().any(|t| sig.has(*t)),
        name,
        version,
        release,
        arch,
    })
}

/// Parse a whole RPM file held in memory (tests, local files).
pub fn parse_file_bytes(buf: &[u8]) -> Result<(PackageInfo, Layout)> {
    match probe(buf)? {
        Probe::NeedBytes(n) => bail!("file too short for an RPM header (needs {n} bytes)"),
        Probe::Layout(l) => Ok((parse_package(buf, &l)?, l)),
    }
}
