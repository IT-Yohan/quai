#!/usr/bin/env bash
# Create or update quai's COPR project, build a source RPM in it and wait for
# the result. Run by the release workflow; works by hand too.
#
#   packaging/copr/submit.sh quai-<version>-1.src.rpm
#
# Needs copr-cli and an API token in ~/.config/copr
# (https://copr.fedorainfracloud.org/api/; it expires after 180 days).
#   COPR_OWNER        default: the token's user
#   COPR_PROJECT      default: quai
#   FEDORA_RELEASES   JSON list of releases for a new project (default ["44","45"]);
#                     COPR then follows Fedora branching and end-of-life by itself.
# Prints owner=, project=, build_id= and succeeded= lines (also to $GITHUB_OUTPUT).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
srpm="$(realpath "$1")"
api=https://copr.fedorainfracloud.org/api_3

owner="${COPR_OWNER:-$(copr-cli whoami)}"
project="${COPR_PROJECT:-quai}"
full="$owner/$project"
emit() { echo "$1"; [[ -z "${GITHUB_OUTPUT:-}" ]] || echo "$1" >> "$GITHUB_OUTPUT"; }

render() { sed -e "s|@OWNER@|$owner|g" -e "s|@PROJECT@|$project|g" "$here/$1"; }
settings=(
  --description "$(render description.md)"
  --instructions "$(render instructions.md)"
  # AppStream metadata in the repository: GNOME Software and Discover list Quai.
  --appstream on
  --follow-fedora-branching on
  --unlisted-on-hp off
  --enable-net off
)

if curl -fsS -o /dev/null "$api/project?ownername=$owner&projectname=$project"; then
  echo "== updating $full"
  # Chroots are left alone: COPR adds new Fedora releases and retires old ones.
  copr-cli modify "$full" "${settings[@]}"
else
  echo "== creating $full"
  chroots=()
  for r in $(python3 -c 'import json, os; print(" ".join(json.loads(os.environ.get("FEDORA_RELEASES") or "[\"44\", \"45\"]")))') rawhide; do
    for a in x86_64 aarch64; do chroots+=(--chroot "fedora-$r-$a"); done
  done
  copr-cli create "$full" "${chroots[@]}" "${settings[@]}"
fi

echo "== building $(basename "$srpm")"
out="$(copr-cli build --nowait "$full" "$srpm")"
echo "$out"
build_id="$(sed -n 's/^Created builds: \([0-9]*\).*/\1/p' <<<"$out")"
[[ -n "$build_id" ]] || { echo "no build id in copr-cli's answer" >&2; exit 1; }
emit "owner=$owner"
emit "project=$project"
emit "build_id=$build_id"
echo "https://copr.fedorainfracloud.org/coprs/$full/build/$build_id/"
copr-cli watch-build "$build_id" || true

# Per chroot: a failure on a released Fedora fails the job, on rawhide it is only reported.
python3 - "$api" "$build_id" <<'PY'
import json, sys, urllib.request
api, build_id = sys.argv[1:]
with urllib.request.urlopen(f"{api}/build-chroot/list?build_id={build_id}&limit=100", timeout=60) as r:
    items = json.load(r)["items"]
bad = []
for c in sorted(items, key=lambda c: c["name"]):
    print(f"  {c['name']:28} {c['state']:10} {c.get('result_url') or ''}")
    if c["state"] != "succeeded" and "rawhide" not in c["name"]:
        bad.append(c["name"])
if bad:
    sys.exit("failed: " + ", ".join(bad))
PY
emit "succeeded=true"
