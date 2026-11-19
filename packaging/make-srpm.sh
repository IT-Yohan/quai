#!/usr/bin/env bash
# Build quai's source RPM from this checkout: the spec, the source tarball
# (git archive of HEAD) and the vendored crates. Used by CI, by the release
# workflow, by COPR (.copr/Makefile) and by hand.
#
#   packaging/make-srpm.sh [--release | --snapshot] [--outdir DIR]
#
#   --release   Version = the version in Cargo.toml. HEAD must be tagged
#               v<version>; the spec's Source URLs point at that GitHub release.
#   --snapshot  Version = <last release>^<date>git<commit> once a release
#               exists, <Cargo version>~<date>git<commit> before the first one,
#               so that dnf orders snapshots and releases correctly.
#               The default unless HEAD is the release tag.
#
# Writes to DIR (default: dist/): the SRPM, the two source archives and the
# spec it used. Needs git, cargo, cargo-vendor-filterer, rpmbuild, python3, xz.
set -euo pipefail

top="$(git -C "$(dirname "$0")/.." rev-parse --show-toplevel)"
cd "$top"
outdir="$top/dist"
mode=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) mode=release ;;
    --snapshot) mode=snapshot ;;
    --outdir) outdir="$(realpath -m "$2")"; shift ;;
    --outdir=*) outdir="$(realpath -m "${1#*=}")" ;;
    -h|--help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

die() { echo "make-srpm: $*" >&2; exit 1; }
for tool in git cargo rpmbuild python3 xz; do
  command -v "$tool" >/dev/null || die "$tool is required"
done
cargo vendor-filterer --version >/dev/null 2>&1 \
  || die "cargo-vendor-filterer is required (dnf install cargo-vendor-filterer, or cargo install cargo-vendor-filterer)"
[[ -z "$(git status --porcelain --untracked-files=no)" ]] \
  || echo "make-srpm: warning: uncommitted changes are not included (git archive of HEAD)" >&2

cargo_version="$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml","rb"))["workspace"]["package"]["version"])')"
commit="$(git rev-parse HEAD)"
short="${commit:0:7}"
# Dates come from the commit, so the same commit always gives the same version.
date="$(git log -1 --format=%cd --date=format:%Y%m%d HEAD)"
tagged=""
git rev-parse -q --verify "refs/tags/v$cargo_version^{commit}" >/dev/null \
  && [[ "$(git rev-parse "v$cargo_version^{commit}")" == "$commit" ]] && tagged=1
[[ -n "$mode" ]] || mode=$([[ -n "$tagged" ]] && echo release || echo snapshot)

if [[ "$mode" == release ]]; then
  [[ -n "$tagged" ]] || die "--release needs HEAD tagged v$cargo_version"
  version="$cargo_version"
else
  last_tag="$(git describe --tags --abbrev=0 --match 'v[0-9]*' HEAD 2>/dev/null || true)"
  if [[ -n "$last_tag" ]]; then
    version="${last_tag#v}^${date}git${short}"
  else
    version="${cargo_version}~${date}git${short}"
  fi
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
src="quai-$version"
mkdir -p "$outdir"

echo "make-srpm: quai $version ($mode, $short)"

# Sources: exactly what is committed.
git archive --format=tar --prefix="$src/" HEAD | gzip -9n > "$work/$src.tar.gz"

# Vendored crates for every architecture Fedora builds Rust on, nothing for
# other systems (the Windows crates alone are ~100 MB).
tar -xzf "$work/$src.tar.gz" -C "$work"
(
  cd "$work/$src"
  platforms=()
  for t in x86_64 i686 aarch64 powerpc64le s390x riscv64gc; do
    platforms+=("--platform=$t-unknown-linux-gnu")
  done
  cargo vendor-filterer --versioned-dirs "${platforms[@]}" vendor >/dev/null
  # Reproducible archive: sorted, fixed owner and time.
  tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$(git -C "$top" log -1 --format=%ct HEAD)" \
    -cf - vendor | xz -T0 -9 > "$work/$src-vendor.tar.xz"
)
rm -rf "${work:?}/$src"

# The spec, with this version.
spec="$work/quai.spec"
python3 - "$top/packaging/quai.spec" "$spec" "$version" "$mode" "$commit" "$date" <<'PY'
import re, sys, datetime
src, dst, version, mode, commit, date = sys.argv[1:]
text = open(src, encoding="utf-8").read()
text, n = re.subn(r"(?m)^Version:(\s*).*$", lambda m: f"Version:{m.group(1)}{version}", text, count=1)
assert n == 1, "no Version: line"
if mode == "snapshot":
    # Snapshot sources are not downloadable: plain file names.
    text = re.sub(r"(?m)^(Source0:\s*).*$", r"\g<1>%{name}-%{version}.tar.gz", text, count=1)
    text = re.sub(r"(?m)^(Source1:\s*).*$", r"\g<1>%{name}-%{version}-vendor.tar.xz", text, count=1)
    day = datetime.datetime.strptime(date, "%Y%m%d").strftime("%a %b %d %Y")
    release = re.search(r"(?m)^Release:\s*(\d+)", text).group(1)
    entry = (f"* {day} quai CI - {version}-{release}\n"
             f"- Snapshot of https://github.com/IT-Yohan/quai/commit/{commit}\n\n")
    text = text.replace("%changelog\n", "%changelog\n" + entry, 1)
open(dst, "w", encoding="utf-8").write(text)
PY

# Source RPM without a dist tag: it rebuilds for any Fedora release.
rpmbuild -bs "$spec" \
  --define "_sourcedir $work" --define "_srcrpmdir $outdir" --define "dist %{nil}" \
  --define "_source_payload w9.xzdio" >/dev/null
cp "$work/$src.tar.gz" "$work/$src-vendor.tar.xz" "$outdir/"
cp "$spec" "$outdir/quai.spec"
srpm="$(ls -t "$outdir"/quai-*.src.rpm | head -1)"
echo "make-srpm: $srpm"
# For CI: the version and the SRPM path.
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  { echo "version=$version"; echo "srpm=$srpm"; } >> "$GITHUB_OUTPUT"
fi
