#!/usr/bin/env bash
# Screenshots for the AppStream metadata (GNOME Software, Discover) and the
# README, rendered from the real GTK and Kirigami windows.
#
# A throw-away setup like tests/e2e/run.sh: a mock forge publishes releases of
# a few real projects that ship RPMs (fake packages with their names), quaid
# serves them, dnf installs some into a scratch root, and an update is waiting.
# Then each frontend is started and captured, in light and dark styles.
#
#   tests/screenshots/run.sh OUTDIR [path/to/target]
#
# Needs root (dnf --installroot), rpmbuild, dnf, python3, curl, Xvfb, xdotool,
# ImageMagick, dbus-run-session, Kirigami with the Breeze style and the KDE
# platform theme, and the fonts and icons of both desktops (see
# .github/workflows/screenshots.yml for the Fedora package list).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="$(realpath -m "$1")"
bin="$(realpath -m "${2:-$here/../../target/release}")"
QUAI="$bin/quai"
FORGE_PORT=18900
QUAID_PORT=18941
API="http://127.0.0.1:$FORGE_PORT"
SCALE="${SCALE:-2}"
mkdir -p "$out"

root="$(mktemp -d /tmp/quai-shots.XXXXXX)"
pids=()
cleanup() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
  [[ -n "${KEEP:-}" ]] || rm -rf "$root"
}
trap cleanup EXIT

export LC_ALL=C.UTF-8 LANGUAGE=
export QUAI_LOCALEDIR="${QUAI_LOCALEDIR:-$here/../../target/generated/locale}"
export QUAI_CONFIG_DIR="$root/etc/quai" QUAI_REPOS_DIR="$root/repos.d" QUAI_CACHE_DIR="$root/cache"
export QUAI_SYSTEMD_DIR="$root/systemd" QUAI_RPM_ROOT="$root/sysroot" QUAI_DIST=fc45
if dnf --version 2>/dev/null | grep -qi dnf5; then flavour="--use-host-config --no-plugins"; else flavour="--noplugins"; fi
export QUAI_DNF_ARGS="--installroot=$root/sysroot --setopt=reposdir=$root/repos.d --releasever=45 $flavour \
--setopt=install_weak_deps=0 --setopt=cachedir=$root/dnfcache"
export QUAI_BIN="$QUAI" QUAI_API_URL="$API" QUAID_FORCE_COOLDOWN_SECS=0
mkdir -p "$root"/{assets,repos.d,sysroot,rpmbuild/SPECS}

say() { printf '== %s\n' "$*"; }

say "packages"
# name version summary url license
mkrpm() {
  cat > "$root/rpmbuild/SPECS/$1-$2.spec" <<SPEC
Name: $1
Version: $2
Release: 1
Summary: $3
URL: $4
License: $5
BuildArch: x86_64
AutoReqProv: no
%description
$3.
%install
mkdir -p %{buildroot}/opt/$1
head -c 2000000 /dev/urandom > %{buildroot}/opt/$1/payload
%files
/opt/$1
SPEC
  rpmbuild --quiet --define "_topdir $root/rpmbuild" --define "_build_id_links none" \
    --define "_binary_payload w1.gzdio" -bb "$root/rpmbuild/SPECS/$1-$2.spec" >/dev/null 2>&1
}
mkrpm codium 1.105.17075 "Code editor built from the open source VS Code sources" https://vscodium.com MIT
mkrpm codium 1.106.37943 "Code editor built from the open source VS Code sources" https://vscodium.com MIT
mkrpm heroic 2.18.1 "Games launcher for GOG, Epic Games and Amazon Prime Games" https://heroicgameslauncher.com GPL-3.0-only
mkrpm ferdium 7.1.1 "All your services in one place" https://ferdium.org Apache-2.0
mkrpm tabby-terminal 1.0.226 "A terminal for a more modern age" https://tabby.sh MIT
mkrpm pulsar 1.130.1 "A community-led hyper-hackable text editor" https://pulsar-edit.dev MIT
find "$root/rpmbuild/RPMS" -name '*.rpm' -exec cp {} "$root/assets/" \;

publish() { python3 - "$root/releases.json" "$@" <<'PY'
import json, sys
path, codium_latest = sys.argv[1], sys.argv[2]
def rel(tag, date, *assets):
    return {"tag_name": tag, "name": tag.lstrip("v"), "draft": False, "prerelease": False,
            "published_at": date, "assets": list(assets)}
codium = [rel("1.105.17075", "2026-10-21T09:00:00Z", "codium-1.105.17075-1.x86_64.rpm")]
if codium_latest == "new":
    codium.insert(0, rel("1.106.37943", "2026-11-17T09:00:00Z", "codium-1.106.37943-1.x86_64.rpm"))
json.dump({
    "VSCodium/vscodium": codium,
    "Heroic-Games-Launcher/HeroicGamesLauncher": [rel("v2.18.1", "2026-11-05T12:00:00Z", "heroic-2.18.1-1.x86_64.rpm")],
    "ferdium/ferdium-app": [rel("v7.1.1", "2026-10-30T15:00:00Z", "ferdium-7.1.1-1.x86_64.rpm")],
    "Eugeny/tabby": [rel("v1.0.226", "2026-11-12T08:00:00Z", "tabby-terminal-1.0.226-1.x86_64.rpm")],
    "pulsar-edit/pulsar": [rel("v1.130.1", "2026-11-03T18:00:00Z", "pulsar-1.130.1-1.x86_64.rpm")],
}, open(path, "w"))
PY
}
publish old
python3 "$here/../e2e/mock_forge.py" --port $FORGE_PORT --dir "$root/assets" --releases "$root/releases.json" &
pids+=($!)

say "quai and quaid"
"$QUAI" config listen 127.0.0.1:$QUAID_PORT --json >/dev/null
"$QUAI" setup --no-systemd --json >/dev/null
"$bin/quaid" --listen 127.0.0.1:$QUAID_PORT > "$root/quaid.log" 2>&1 &
pids+=($!)
for _ in $(seq 50); do curl -fsS "http://127.0.0.1:$QUAID_PORT/-/health" >/dev/null 2>&1 && break; sleep 0.1; done

add() { # project name [--install]
  "$QUAI" add "$1" --name "$2" --yes "${@:3}" --json > "$root/add.json" || { cat "$root/add.json"; exit 1; }
}
add VSCodium/vscodium VSCodium --install
add Heroic-Games-Launcher/HeroicGamesLauncher "Heroic Games Launcher" --install
add ferdium/ferdium-app Ferdium --install
add Eugeny/tabby Tabby
publish new
"$QUAI" refresh --json >/dev/null
"$QUAI" status --json > "$root/status.json"
python3 - "$root/status.json" <<'PY'
import json, sys
for s in json.load(open(sys.argv[1]))["sources"]:
    print(f"   {s['source']['name']:24} {s['state']}")
PY

# One display for the GTK window; the Kirigami one renders offscreen.
export DISPLAY=:77
Xvfb "$DISPLAY" -screen 0 3200x2400x24 -nolisten tcp >/dev/null 2>&1 &
pids+=($!)
sleep 1

# GNOME's defaults: Adwaita fonts and icons, only a close button.
export XDG_CONFIG_HOME="$root/config"
mkdir -p "$XDG_CONFIG_HOME/gtk-4.0"
cat > "$XDG_CONFIG_HOME/gtk-4.0/settings.ini" <<'INI'
[Settings]
gtk-decoration-layout=appmenu:close
gtk-font-name=Adwaita Sans 11
gtk-icon-theme-name=Adwaita
INI

gtk_shot() { # name scheme [env...] -- keys...
  local name="$1" scheme="$2"; shift 2
  local envs=() keys=()
  while [[ $# -gt 0 && "$1" != -- ]]; do envs+=("$1"); shift; done
  [[ "${1:-}" == -- ]] && shift
  keys=("$@")
  say "GTK: $name ($scheme)"
  env GDK_BACKEND=x11 GSK_RENDERER=cairo GDK_SCALE="$SCALE" NO_AT_BRIDGE=1 ADW_DEBUG_COLOR_SCHEME="$scheme" \
    "${envs[@]}" dbus-run-session -- "$bin/quai-gtk" > "$root/gtk-$name.log" 2>&1 &
  local pid=$!
  local wid
  wid="$(timeout 30 xdotool search --sync --onlyvisible --name '^Quai$' | head -1)"
  xdotool windowsize "$wid" $((900 * SCALE)) $((620 * SCALE))
  sleep 3
  for k in "${keys[@]}"; do
    case "$k" in
      type:*) xdotool type --window "$wid" --delay 30 "${k#type:}" ;;
      sleep:*) sleep "${k#sleep:}" ;;
      *) xdotool key --window "$wid" "$k" ;;
    esac
    sleep 0.5
  done
  sleep 2
  import -window "$wid" "$out/$name.png"
  # dbus-run-session does not pass signals on: close the window itself.
  xdotool windowkill "$wid" 2>/dev/null || true
  pkill -f "^$bin/quai-gtk" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  while xdotool search --onlyvisible --name '^Quai$' >/dev/null 2>&1; do sleep 0.2; done
}

gtk_shot gnome-sources prefer-light
gtk_shot gnome-source prefer-light QUAI_GTK_OPEN=vscodium
gtk_shot gnome-add prefer-light -- ctrl+n sleep:1 type:pulsar-edit/pulsar Return sleep:4
gtk_shot gnome-sources-dark prefer-dark

say "Kirigami"
mkdir -p "$root/kde"
# Breeze, as on a Plasma desktop; BreezeDark for the dark captures.
kde_shots() { # suffix colors
  local cfg="$root/kde/$1"
  mkdir -p "$cfg"
  { [[ -n "$2" ]] && cat "$2"; printf '\n[General]\nfont=Noto Sans,10,-1,5,400,0,0,0,0,0,0,0,0,0,0,1\n'
    printf '[Icons]\nTheme=breeze%s\n' "$([[ "$1" == -dark ]] && echo -dark)"; } > "$cfg/kdeglobals"
  sed -e "s|@OUT@|$out|" -e "s|@SUFFIX@|$1|" "$here/kde-shots.qml" > "$cfg/shots.qml"
  XDG_CONFIG_HOME="$cfg" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=kde QT_QUICK_BACKEND=software \
    QT_SCALE_FACTOR="$SCALE" QUAI_KDE_MAIN="$cfg/shots.qml" \
    timeout 120 "$bin/quai-kde" > "$root/kde$1.log" 2>&1 || true
  grep -E "^qml: shot" "$root/kde$1.log" | sed 's/^qml: /   /' || true
  grep -q "^qml: SHOTS DONE" "$root/kde$1.log" || { tail -30 "$root/kde$1.log"; exit 1; }
}
kde_shots "" ""
dark=/usr/share/color-schemes/BreezeDark.colors
if [[ -f "$dark" ]]; then kde_shots -dark "$dark"; else echo "   no $dark: no dark Plasma captures"; fi

say "optimising"
if command -v oxipng >/dev/null; then oxipng -q -o 3 --strip safe "$out"/*.png; fi
ls -l "$out"
