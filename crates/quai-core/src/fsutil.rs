//! Small filesystem helpers: atomic writes and ownership.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Write `data` to `path` atomically (temp file + rename) with `mode`.
pub fn write_atomic(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    let dir = path.parent().context("path has no parent directory")?;
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let file_name = path.file_name().context("path has no file name")?.to_string_lossy();
    let tmp = dir.join(format!(".{file_name}.tmp-{}", std::process::id()));
    {
        let mut f = fs::File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
        f.write_all(data)?;
        f.sync_all().ok();
    }
    fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

pub fn set_mode(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("chmod {:o} {}", mode, path.display()))
}

/// Look up a group id by name (`getgrnam`).
pub fn group_id(name: &str) -> Option<u32> {
    let cname = std::ffi::CString::new(name).ok()?;
    // SAFETY: getgrnam returns a pointer to static storage or NULL; we copy the gid out immediately.
    unsafe {
        let gr = libc::getgrnam(cname.as_ptr());
        if gr.is_null() {
            None
        } else {
            Some((*gr).gr_gid)
        }
    }
}

/// `chown root:<group>` when running as root and the group exists; no-op otherwise.
pub fn chown_root_group(path: &Path, group: &str) -> Result<()> {
    if !crate::system::is_root() {
        return Ok(());
    }
    let Some(gid) = group_id(group) else {
        return Ok(());
    };
    std::os::unix::fs::chown(path, Some(0), Some(gid)).with_context(|| format!("chown root:{group} {}", path.display()))
}
