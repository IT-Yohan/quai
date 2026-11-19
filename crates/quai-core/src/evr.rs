//! RPM version comparison (`rpmvercmp`) and EVR helpers.

use std::cmp::Ordering;

pub fn format_evr(epoch: u32, version: &str, release: &str) -> String {
    let vr = if release.is_empty() { version.to_string() } else { format!("{version}-{release}") };
    if epoch > 0 {
        format!("{epoch}:{vr}")
    } else {
        vr
    }
}

/// Split `[epoch:]version[-release]`.
pub fn parse_evr(s: &str) -> (u32, String, String) {
    let (epoch, rest) = match s.split_once(':') {
        Some((e, r)) if e.chars().all(|c| c.is_ascii_digit()) && !e.is_empty() => (e.parse().unwrap_or(0), r),
        _ => (0, s),
    };
    match rest.rsplit_once('-') {
        Some((v, r)) => (epoch, v.to_string(), r.to_string()),
        None => (epoch, rest.to_string(), String::new()),
    }
}

/// Compare two EVR strings the way rpm does.
pub fn compare_evr(a: &str, b: &str) -> Ordering {
    let (ea, va, ra) = parse_evr(a);
    let (eb, vb, rb) = parse_evr(b);
    ea.cmp(&eb).then_with(|| rpmvercmp(&va, &vb)).then_with(|| {
        if ra.is_empty() || rb.is_empty() {
            Ordering::Equal
        } else {
            rpmvercmp(&ra, &rb)
        }
    })
}

/// Port of rpm's `rpmvercmp()`, including `~` (sorts before) and `^` (sorts after).
pub fn rpmvercmp(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    let a = a.as_bytes();
    let b = b.as_bytes();
    let (mut i, mut j) = (0usize, 0usize);
    let sep = |c: u8| !c.is_ascii_alphanumeric() && c != b'~' && c != b'^';

    while i < a.len() || j < b.len() {
        while i < a.len() && sep(a[i]) {
            i += 1;
        }
        while j < b.len() && sep(b[j]) {
            j += 1;
        }

        // Tilde sorts before everything, even the end of the string.
        let at = |s: &[u8], k: usize, c: u8| k < s.len() && s[k] == c;
        if at(a, i, b'~') || at(b, j, b'~') {
            if !at(a, i, b'~') {
                return Ordering::Greater;
            }
            if !at(b, j, b'~') {
                return Ordering::Less;
            }
            i += 1;
            j += 1;
            continue;
        }

        // Caret sorts after the end of the string but before anything else.
        if at(a, i, b'^') || at(b, j, b'^') {
            if i >= a.len() {
                return Ordering::Less;
            }
            if j >= b.len() {
                return Ordering::Greater;
            }
            if !at(a, i, b'^') {
                return Ordering::Greater;
            }
            if !at(b, j, b'^') {
                return Ordering::Less;
            }
            i += 1;
            j += 1;
            continue;
        }

        if i >= a.len() || j >= b.len() {
            break;
        }

        let numeric = a[i].is_ascii_digit();
        let take = |s: &[u8], mut k: usize| {
            let start = k;
            while k < s.len() && if numeric { s[k].is_ascii_digit() } else { s[k].is_ascii_alphabetic() } {
                k += 1;
            }
            (start, k)
        };
        let (sa, ea) = take(a, i);
        let (sb, eb) = take(b, j);
        if sb == eb {
            // Segments of different kinds: numeric is newer.
            return if numeric { Ordering::Greater } else { Ordering::Less };
        }
        let mut seg_a = &a[sa..ea];
        let mut seg_b = &b[sb..eb];
        if numeric {
            while seg_a.len() > 1 && seg_a[0] == b'0' {
                seg_a = &seg_a[1..];
            }
            while seg_b.len() > 1 && seg_b[0] == b'0' {
                seg_b = &seg_b[1..];
            }
            match seg_a.len().cmp(&seg_b.len()) {
                Ordering::Equal => {}
                other => return other,
            }
        }
        match seg_a.cmp(seg_b) {
            Ordering::Equal => {}
            other => return other,
        }
        i = ea;
        j = eb;
    }

    match (i >= a.len(), j >= b.len()) {
        (true, true) => Ordering::Equal,
        (false, _) => Ordering::Greater,
        _ => Ordering::Less,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ordering::*;

    #[test]
    fn vercmp_matches_rpm() {
        let cases = [
            ("1.0", "1.0", Equal),
            ("1.0", "2.0", Less),
            ("2.0.1", "2.0", Greater),
            ("1.10", "1.9", Greater),
            ("1.0a", "1.0", Greater),
            ("1.0~rc1", "1.0", Less),
            ("1.0~rc1", "1.0~rc2", Less),
            ("1.0^git1", "1.0", Greater),
            ("1.0^git1", "1.0.1", Less),
            ("1.0", "1.0^", Less),
            ("abc", "abd", Less),
            ("1.001", "1.1", Equal),
            ("1.a", "1.1", Less),
            ("5.5p1", "5.5p10", Less),
            ("1.4.239", "1.4.240", Less),
        ];
        for (a, b, want) in cases {
            assert_eq!(rpmvercmp(a, b), want, "{a} vs {b}");
            assert_eq!(rpmvercmp(b, a), want.reverse(), "{b} vs {a}");
        }
    }

    #[test]
    fn evr_parsing_and_comparison() {
        assert_eq!(parse_evr("2:1.0-3.fc43"), (2, "1.0".into(), "3.fc43".into()));
        assert_eq!(parse_evr("1.0"), (0, "1.0".into(), String::new()));
        assert_eq!(compare_evr("1:1.0-1", "2.0-1"), Greater);
        assert_eq!(compare_evr("1.4.239-1", "1.4.240-1"), Less);
        assert_eq!(format_evr(0, "1.0", "1"), "1.0-1");
        assert_eq!(format_evr(3, "1.0", ""), "3:1.0");
    }
}
