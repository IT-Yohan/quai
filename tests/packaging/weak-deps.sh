#!/usr/bin/env bash
# Check that installing quai picks the frontend matching the desktop, using
# empty packages that carry the dependencies of packaging/quai.spec.
# Needs rpmbuild, createrepo_c and dnf; run as root (dnf --installroot).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
spec="$here/../../packaging/quai.spec"
root="$(mktemp -d /tmp/quai-weakdeps.XXXXXX)"
trap '[[ -n "${KEEP:-}" ]] || rm -rf "$root"' EXIT
mkdir -p "$root/rpmbuild/SPECS" "$root/repo"

# Dependency lines of one (sub)package section of the real spec, with the
# macros that matter expanded and system packages dropped.
deps() { # "main" or a section-header regex
  awk -v hdr="$1" '
    BEGIN { on = (hdr == "main") }
    $0 ~ "^%package|^%description|^%prep" { on = ($0 ~ hdr) }
    on && /^(Requires|Recommends|Supplements):/ { print }
  ' "$spec" | sed -e 's/%{name}/quai/g' -e 's/%{?_isa}//g' -e 's/ = %{version}-%{release}//g' \
    | grep -vE "^(Requires|Recommends): +(\(dnf5|systemd|polkit|kf6-|hicolor-icon-theme|qt6-)"
}
mk() { # name deps arch
  cat > "$root/rpmbuild/SPECS/$1.spec" <<SPEC
Name: $1
Version: 1
Release: 1
Summary: $1
License: MIT
BuildArch: $3
$2
%description
stub
%files
SPEC
  rpmbuild --quiet --define "_topdir $root/rpmbuild" -bb "$root/rpmbuild/SPECS/$1.spec" >"$root/build-$1.log" 2>&1 || { cat "$root/build-$1.log"; exit 1; }
}
mk quai "$(deps main)" x86_64
mk quai-gui "$(deps '^%package gui')" noarch
mk quai-gtk "$(deps '^%package gtk')" x86_64
mk quai-kde "$(deps '^%package kde')" x86_64
mk gnome-shell "" x86_64
mk plasma-workspace "" x86_64
mk xfce4-session "" x86_64
find "$root/rpmbuild/RPMS" -name '*.rpm' -exec cp {} "$root/repo/" \;
createrepo_c -q "$root/repo"

if dnf --version 2>/dev/null | grep -qi dnf5; then flavour="--use-host-config --no-plugins"; else flavour="--noplugins"; fi
fail=0
n=0
scenario() { # label desktop-packages install expected-quai-packages
  n=$((n + 1))
  local r="$root/sys-$n"
  local args="--installroot=$r --releasever=43 $flavour --disablerepo=* --repofrompath=stubs,$root/repo --setopt=stubs.gpgcheck=0 --setopt=cachedir=$r/cache -y -q"
  # shellcheck disable=SC2086
  [[ -z "$2" ]] || dnf $args install $2 >"$r.log" 2>&1 || { cat "$r.log"; exit 1; }
  # shellcheck disable=SC2086
  dnf $args install $3 >"$r.log" 2>&1 || { cat "$r.log"; exit 1; }
  local got
  got="$(rpm --root "$r" -qa --qf '%{name}\n' | grep '^quai' | sort | xargs)"
  if [[ "$got" == "$4" ]]; then
    printf '  \033[32m✓\033[0m %-34s → %s\n' "$1" "$got"
  else
    printf '  \033[31m✗ %-34s → %s (expected %s)\033[0m\n' "$1" "$got" "$4"
    fail=1
  fi
}
scenario "GNOME: install quai" gnome-shell quai "quai quai-gtk quai-gui"
scenario "Plasma: install quai" plasma-workspace quai "quai quai-gui quai-kde"
scenario "GNOME + Plasma: install quai" "gnome-shell plasma-workspace" quai "quai quai-gtk quai-gui quai-kde"
scenario "no desktop: install quai" "" quai "quai"
scenario "Xfce: install quai-gui" xfce4-session quai-gui "quai quai-gtk quai-gui"
scenario "Plasma: install quai-gtk" plasma-workspace quai-gtk "quai quai-gtk quai-gui quai-kde"
exit $fail
