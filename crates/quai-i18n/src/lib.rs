//! Translations for every quai program, through the C library's gettext.
//!
//! Messages are written in English in the code and looked up in the `quai`
//! gettext domain, so a new language is one `po/<lang>.po` file and no code.
//! Placeholders are named (`{id}`), never positional, so a translation can
//! reorder them; the catalogue marks those messages `python-brace-format`,
//! which makes `msgfmt --check` reject a translation that drops or renames one.
//!
//! The functions below are also the extraction keywords of `po/extract.py`:
//! their message arguments must be string literals.
//!
//! | Function | Use |
//! | --- | --- |
//! | [`tr`] | a fixed message |
//! | [`trf`] | a message with `{name}` placeholders |
//! | [`trn`] / [`trnf`] | a message that depends on a count (`{n}` is filled in) |
//! | [`trc`] | a short message that needs a context to be translated right |
//! | [`n_`] | marks a message translated later with `tr(variable)` |

use std::ffi::{CStr, CString};
use std::fmt::Display;
use std::os::raw::{c_char, c_ulong};
use std::path::PathBuf;
use std::sync::Once;

/// The gettext domain: `<localedir>/<lang>/LC_MESSAGES/quai.mo`.
pub const DOMAIN: &str = "quai";

extern "C" {
    fn bindtextdomain(domain: *const c_char, dir: *const c_char) -> *mut c_char;
    fn bind_textdomain_codeset(domain: *const c_char, codeset: *const c_char) -> *mut c_char;
    fn dgettext(domain: *const c_char, msgid: *const c_char) -> *mut c_char;
    fn dngettext(domain: *const c_char, msgid: *const c_char, plural: *const c_char, n: c_ulong) -> *mut c_char;
}

static INIT: Once = Once::new();

/// Where the catalogues are: `$QUAI_LOCALEDIR` (development and tests), else
/// the `QUAI_LOCALEDIR` given at build time, else `/usr/share/locale`.
pub fn locale_dir() -> PathBuf {
    std::env::var_os("QUAI_LOCALEDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(option_env!("QUAI_LOCALEDIR").unwrap_or("/usr/share/locale")))
}

/// Adopt the user's locale (`LANG`, `LC_ALL`, `LANGUAGE`…) and bind the
/// catalogue. Call once at the start of `main`; until then every function
/// returns the English text.
pub fn init() {
    INIT.call_once(|| {
        let domain = CString::new(DOMAIN).unwrap();
        let dir = CString::new(locale_dir().into_os_string().into_encoded_bytes()).unwrap_or_default();
        // SAFETY: plain C library calls with NUL-terminated strings that outlive them.
        unsafe {
            libc::setlocale(libc::LC_ALL, c"".as_ptr());
            bindtextdomain(domain.as_ptr(), dir.as_ptr());
            bind_textdomain_codeset(domain.as_ptr(), c"UTF-8".as_ptr());
        }
    });
}

fn lookup(msgid: &str) -> String {
    let (Ok(domain), Ok(id)) = (CString::new(DOMAIN), CString::new(msgid)) else {
        return msgid.to_string();
    };
    // SAFETY: dgettext returns either `id` itself or a string owned by the
    // loaded catalogue; both stay valid until the copy below is made.
    unsafe { CStr::from_ptr(dgettext(domain.as_ptr(), id.as_ptr())).to_string_lossy().into_owned() }
}

fn lookup_plural(singular: &str, plural: &str, n: u64) -> String {
    let (Ok(domain), Ok(one), Ok(many)) = (CString::new(DOMAIN), CString::new(singular), CString::new(plural)) else {
        return if n == 1 { singular } else { plural }.to_string();
    };
    // Counts beyond the C type select the last plural form, as gettext would.
    let n = c_ulong::try_from(n).unwrap_or(c_ulong::MAX);
    // SAFETY: as in `lookup`.
    unsafe { CStr::from_ptr(dngettext(domain.as_ptr(), one.as_ptr(), many.as_ptr(), n)).to_string_lossy().into_owned() }
}

/// Translate a fixed message.
pub fn tr(msgid: &str) -> String {
    lookup(msgid)
}

/// Translate a message whose meaning depends on `context` (gettext's `msgctxt`),
/// e.g. `trc("package state", "Installed")`.
pub fn trc(context: &str, msgid: &str) -> String {
    let key = format!("{context}\u{4}{msgid}");
    let found = lookup(&key);
    if found == key {
        msgid.to_string()
    } else {
        found
    }
}

/// Translate a message, then fill its `{name}` placeholders.
pub fn trf(msgid: &str, args: &[(&str, &dyn Display)]) -> String {
    fill(&lookup(msgid), args)
}

/// Translate a message that depends on a count; `{n}` is replaced by `n`.
pub fn trn(singular: &str, plural: &str, n: u64) -> String {
    fill(&lookup_plural(singular, plural, n), &[("n", &n)])
}

/// [`trn`] with more placeholders than `{n}`.
pub fn trnf(singular: &str, plural: &str, n: u64, args: &[(&str, &dyn Display)]) -> String {
    let text = fill(&lookup_plural(singular, plural, n), args);
    fill(&text, &[("n", &n)])
}

/// Mark a message for extraction without translating it yet: for tables of
/// messages translated where they are shown, with `tr(entry)`.
pub const fn n_(msgid: &str) -> &str {
    msgid
}

/// Replace each `{name}` of `template` by its value in `args`. Unknown names
/// and lone braces are left as they are; values are inserted verbatim, so a
/// value containing `{x}` is never expanded again.
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let name_len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
        let value = (name_len > 0 && after[name_len..].starts_with('}'))
            .then(|| args.iter().find(|(k, _)| *k == &after[..name_len]))
            .flatten();
        match value {
            Some((_, v)) => {
                out.push_str(&v.to_string());
                rest = &after[name_len + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_named_placeholders_once() {
        assert_eq!(fill("Added {id} ({n} packages)", &[("id", &"orca"), ("n", &2)]), "Added orca (2 packages)");
        assert_eq!(fill("{a}{a}", &[("a", &"x")]), "xx");
        assert_eq!(fill("keep {unknown} and {", &[]), "keep {unknown} and {");
        assert_eq!(fill("{v}", &[("v", &"{v}")]), "{v}");
        assert_eq!(fill("regex {1,3} stays", &[]), "regex {1,3} stays");
        assert_eq!(fill("ünïcode {x}é", &[("x", &"→")]), "ünïcode →é");
    }

    #[test]
    fn untranslated_lookups_return_english() {
        assert_eq!(tr("Check now"), "Check now");
        assert_eq!(trc("verb", "Update"), "Update");
        assert_eq!(trn("{n} source", "{n} sources", 1), "1 source");
        assert_eq!(trn("{n} source", "{n} sources", 3), "3 sources");
        assert_eq!(trnf("{n} package in {id}", "{n} packages in {id}", 2, &[("id", &"orca")]), "2 packages in orca");
        assert_eq!(tr("bad\0nul"), "bad\0nul");
    }
}
