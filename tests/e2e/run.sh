#!/usr/bin/env bash
# End-to-end test: mock forge -> quaid -> real dnf, in a throw-away tree.
#
# Requires: rpmbuild, dnf (4 or 5), python3, gpg not needed. Run as root
# (dnf --installroot needs it). Usage: tests/e2e/run.sh [path/to/target/debug]
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
bin="$(realpath "${1:-$here/../../target/debug}")"
QUAI="$bin/quai"
QUAID="$bin/quaid"
FORGE_PORT=18800
QUAID_PORT=18741
API="http://127.0.0.1:$FORGE_PORT"

root="$(mktemp -d /tmp/quai-e2e.XXXXXX)"
pids=()
cleanup() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
  [[ -n "${KEEP:-}" ]] || rm -rf "$root"
}
trap cleanup EXIT

export QUAI_CONFIG_DIR="$root/etc/quai" QUAI_REPOS_DIR="$root/repos.d" QUAI_CACHE_DIR="$root/cache"
export QUAI_SYSTEMD_DIR="$root/systemd" QUAI_RPM_ROOT="$root/sysroot"
if dnf --version 2>/dev/null | grep -qi dnf5; then
  # dnf5 reads config and repos from the installroot unless told otherwise.
  dnf_flavour="--use-host-config --no-plugins"
else
  dnf_flavour="--noplugins"
fi
export QUAI_DNF_ARGS="--installroot=$root/sysroot --setopt=reposdir=$root/repos.d --releasever=43 $dnf_flavour --setopt=install_weak_deps=0 --setopt=cachedir=$root/dnfcache"
mkdir -p "$root"/{assets,repos.d,sysroot,rpmbuild/SPECS}

pass=0
ok() { pass=$((pass + 1)); printf '  \033[32m✓\033[0m %s\n' "$*"; }
die() { printf '  \033[31m✗ %s\033[0m\n' "$*"; exit 1; }
jq_() { python3 -c "import json,sys; d=json.load(sys.stdin); print($1)"; }
dnf_() { dnf $QUAI_DNF_ARGS "$@"; }

echo "== building test packages"
mkspec() { # name version release extra-preamble
  cat > "$root/rpmbuild/SPECS/$1-$2-$3.spec" <<SPEC
Name: $1
Version: $2
Release: $3
Summary: quai e2e package $1
License: MIT
BuildArch: x86_64
AutoReqProv: no
$4
%description
Test package.
%install
mkdir -p %{buildroot}/opt/$1
echo $2 > %{buildroot}/opt/$1/VERSION
%files
/opt/$1
SPEC
  rpmbuild --quiet --define "_topdir $root/rpmbuild" --define "_build_id_links none" \
    -bb "$root/rpmbuild/SPECS/$1-$2-$3.spec" >/dev/null 2>&1
}
mkspec app 1.0 1 ""
mkspec app 1.1 1 ""
mkspec app-extras 1.1 1 ""
mkspec app 1.2 1 "Obsoletes: bash"
mkspec multi 2.0 1.el9 ""
mkspec multi 2.0 1.fc43 ""
find "$root/rpmbuild/RPMS" -name '*.rpm' -exec cp {} "$root/assets/" \;
ls "$root/assets" | sed 's/^/   /'

publish() { # json
  echo "$1" > "$root/releases.json"
}
publish '{"test/app": [
  {"tag_name": "v1.0", "name": "1.0", "draft": false, "prerelease": false, "published_at": "2026-11-01T10:00:00Z",
   "assets": ["app-1.0-1.x86_64.rpm"]}],
 "test/variants": [
  {"tag_name": "v2.0", "name": "2.0", "draft": false, "prerelease": false, "published_at": "2026-11-01T10:00:00Z",
   "assets": ["multi-2.0-1.el9.x86_64.rpm", "multi-2.0-1.fc43.x86_64.rpm"]}],
 "test/appimage-only": [
  {"tag_name": "v1", "draft": false, "prerelease": false, "published_at": "2026-11-01T10:00:00Z", "assets": []}]}'

python3 "$here/mock_forge.py" --port $FORGE_PORT --dir "$root/assets" --releases "$root/releases.json" &
pids+=($!)
sleep 0.5

echo "== setup"
"$QUAI" config listen 127.0.0.1:$QUAID_PORT --json >/dev/null
"$QUAI" setup --no-systemd --json > "$root/setup.json"
[[ -s "$QUAI_CONFIG_DIR/repo-key.asc" ]] || die "no public key"
[[ "$(stat -c %a "$QUAI_CONFIG_DIR/private")" == 750 ]] || die "private dir is not 0750"
ok "setup created the configuration and the signing key"

QUAID_FORCE_COOLDOWN_SECS=0 "$QUAID" --listen 127.0.0.1:$QUAID_PORT > "$root/quaid.log" 2>&1 &
pids+=($!)
sleep 0.5
curl -fsS "http://127.0.0.1:$QUAID_PORT/-/health" >/dev/null || die "quaid does not answer"
ok "quaid answers on 127.0.0.1:$QUAID_PORT"

echo "== preview"
"$QUAI" preview test/app --api-url "$API" --json > "$root/preview.json"
[[ "$(jq_ "d['packages'][0]['name'] + ' ' + d['packages'][0]['evr'] + ' ' + d['packages'][0]['checksum']" < "$root/preview.json")" == "app 1.0-1 forge" ]] \
  || die "unexpected preview: $(cat "$root/preview.json")"
ok "preview finds app 1.0-1 with the forge's SHA-256"

"$QUAI" preview test/variants --api-url "$API" --json > "$root/variants.json"
[[ "$(jq_ "len(d['variants'][0]['assets'])" < "$root/variants.json")" == 2 ]] || die "variants not detected"
ok "preview detects two variants of the same package"

if "$QUAI" add test/variants --api-url "$API" --yes --json > "$root/variants-add.json"; then die "add without a pattern should fail"; fi
grep -q "asset-pattern" "$root/variants-add.json" || die "no pattern suggestion"
ok "add refuses ambiguous variants and suggests --asset-pattern"

if "$QUAI" preview test/appimage-only --api-url "$API" --json > "$root/none.json"; then die "should fail"; fi
[[ "$(jq_ "d['kind']" < "$root/none.json")" == not_found ]] || die "wrong error kind"
ok "a project without RPMs gets a clear not_found error"

echo "== add"
"$QUAI" add test/app --api-url "$API" --yes --json > "$root/add.json" || die "add failed: $(cat "$root/add.json")"
grep -q '^repo_gpgcheck=1' "$QUAI_REPOS_DIR/quai-app.repo" || die "repo file not signed-checked"
grep -q '^includepkgs=app$' "$QUAI_REPOS_DIR/quai-app.repo" || die "includepkgs missing"
ok "add wrote sources.d/app.toml and quai-app.repo (repo_gpgcheck=1, includepkgs=app)"
ok "dnf validated the signed metadata: $(jq_ "[x for x in d['details'] if x.startswith('dnf')][0]" < "$root/add.json")"

dnf_ -q repoquery --repo=quai-app --qf '%{name}-%{evr}.%{arch}\n' > "$root/rq.txt" 2>/dev/null
grep -qx 'app-1.0-1.x86_64' "$root/rq.txt" || die "repoquery: $(cat "$root/rq.txt")"
ok "dnf repoquery sees app-1.0-1.x86_64 through quaid"

echo "== install through the 302 redirect"
dnf_ -y -q install app >/dev/null 2>&1 || die "dnf install failed"
rpm --root "$root/sysroot" -q app | grep -qx 'app-1.0-1.x86_64' || die "app not installed"
ok "dnf installed app 1.0 (download redirected to the forge, SHA-256 checked)"
[[ "$("$QUAI" status app --json | jq_ "d['sources'][0]['state']")" == up_to_date ]] || die "status should be up_to_date"
ok "quai status: up_to_date"

echo "== new upstream release"
publish '{"test/app": [
  {"tag_name": "v1.2", "draft": false, "prerelease": false, "published_at": "2026-11-03T10:00:00Z",
   "assets": ["app-1.2-1.x86_64.rpm"]},
  {"tag_name": "v1.1", "draft": false, "prerelease": false, "published_at": "2026-11-02T10:00:00Z",
   "assets": ["app-1.1-1.x86_64.rpm", "app-extras-1.1-1.x86_64.rpm"]},
  {"tag_name": "v1.0", "draft": false, "prerelease": false, "published_at": "2026-11-01T10:00:00Z",
   "assets": ["app-1.0-1.x86_64.rpm"]},
  {"tag_name": "v2.0-rc1", "draft": false, "prerelease": true, "published_at": "2026-11-04T10:00:00Z",
   "assets": ["app-1.2-1.x86_64.rpm"]}]}'
"$QUAI" refresh app --json > "$root/refresh.json"
"$QUAI" status app --json > "$root/status.json"
[[ "$(jq_ "d['sources'][0]['state']" < "$root/status.json")" == update_available ]] || die "no update seen: $(cat "$root/status.json")"
[[ "$(jq_ "d['sources'][0]['packages'][0]['available']" < "$root/status.json")" == 1.1-1 ]] || die "1.1 should be newest served"
ok "refresh sees 1.1; the pre-release is ignored on the stable channel"
jq_ "'\n'.join(x['name'] + ': ' + x['reason'] for x in d['sources'][0]['excluded'])" < "$root/status.json" > "$root/excluded.txt"
grep -q "^app: obsoletes 'bash'" "$root/excluded.txt" || die "obsoletes guard: $(cat "$root/excluded.txt")"
grep -q "^app-extras: not in this source's package list" "$root/excluded.txt" || die "allowlist guard"
ok "app 1.2 (Obsoletes: bash) and app-extras (not allowed) are excluded and reported"

"$QUAI" upgrade app --json > "$root/upgrade.json" || die "upgrade failed: $(cat "$root/upgrade.json")"
rpm --root "$root/sysroot" -q app | grep -qx 'app-1.1-1.x86_64' || die "app not upgraded"
ok "quai upgrade: $(jq_ "d['details'][-1]" < "$root/upgrade.json")"

dnf_ -y -q downgrade app >/dev/null 2>&1 || die "downgrade failed"
rpm --root "$root/sysroot" -q app | grep -qx 'app-1.0-1.x86_64' || die "downgrade did not happen"
ok "dnf downgrade works (keep_versions=3 keeps 1.0 available)"

echo "== tampering"
repodata="$QUAI_CACHE_DIR/repos/app/repodata"
cp "$repodata/repomd.xml" "$root/repomd.good"
sed -i 's#<revision>#<revision>9#' "$repodata/repomd.xml"
if dnf_ -q makecache --refresh --repo=quai-app --setopt=quai-app.skip_if_unavailable=0 > "$root/tamper.txt" 2>&1; then die "dnf accepted tampered metadata"; fi
grep -qi "signature" "$root/tamper.txt" || die "unexpected failure: $(cat "$root/tamper.txt")"
ok "dnf rejects metadata that does not match quai's signature ($(grep -oi 'bad [a-z]* signature' "$root/tamper.txt" | head -1))"
cp "$root/repomd.good" "$repodata/repomd.xml"
dnf_ -q makecache --refresh --repo=quai-app --setopt=quai-app.skip_if_unavailable=0 >/dev/null 2>&1 || die "restored metadata refused"

echo "== set, disable, export, import, remove"
"$QUAI" set app allow-package app-extras --json >/dev/null
grep -q '^includepkgs=app,app-extras$' "$QUAI_REPOS_DIR/quai-app.repo" || die "includepkgs not updated"
dnf_ -q repoquery --refresh --repo=quai-app --qf '%{name}\n' 2>/dev/null | grep -qx app-extras || die "app-extras not served"
ok "allow-package app-extras: served and listed in includepkgs"

"$QUAI" disable app --json >/dev/null
grep -q '^enabled=0' "$QUAI_REPOS_DIR/quai-app.repo" || die "not disabled"
code=$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$QUAID_PORT/repos/app/repodata/repomd.xml")
[[ "$code" == 404 ]] || die "disabled source still served ($code)"
"$QUAI" enable app --json >/dev/null
ok "disable/enable toggle the repo file and quaid"

"$QUAI" export -o "$root/export.toml" --json >/dev/null
grep -q 'project = "test/app"' "$root/export.toml" || die "export incomplete"
"$QUAI" remove app --uninstall --yes --json > "$root/remove.json"
if rpm --root "$root/sysroot" -q app >/dev/null 2>&1; then die "app still installed"; fi
[[ ! -e "$QUAI_REPOS_DIR/quai-app.repo" ]] || die "repo file left behind"
ok "remove --uninstall cleans package, source and repo file"
"$QUAI" import "$root/export.toml" --install --yes --json > "$root/import.json" || die "import: $(cat "$root/import.json")"
rpm --root "$root/sysroot" -q app | grep -qx 'app-1.1-1.x86_64' || die "import --install did not install 1.1"
ok "import --install restores the source and installs app 1.1"

echo "== doctor"
"$QUAI" doctor --json > "$root/doctor.json" || true
python3 - "$root/doctor.json" <<'PY' || die "doctor reported failures"
import json, sys
d = json.load(open(sys.argv[1]))
# app 1.2 (Obsoletes: bash) is expected to be reported as excluded.
expected = lambda c: c["id"] == "source-app" and "obsoletes 'bash'" in c["detail"]
bad = [c for c in d["checks"] if not c["ok"] and not expected(c)]
for c in d["checks"]:
    print(f"     {'✓' if c['ok'] else '✗'} {c['title']}: {c['detail']}")
sys.exit(1 if bad else 0)
PY
ok "doctor is green apart from the expected exclusion of app 1.2"

echo
echo "All $pass checks passed."
