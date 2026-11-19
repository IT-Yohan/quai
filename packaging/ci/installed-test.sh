#!/usr/bin/env bash
# Install the RPMs mock built and check them as a user would get them:
# files, users, translations, Provides, desktop data, then the end-to-end
# test (real dnf, quaid, the Kirigami window) against the installed binaries.
#
#   packaging/ci/installed-test.sh RPMDIR        (as root, in a Fedora container)
set -euo pipefail
dir="$(realpath "$1")"
top="$(cd "$(dirname "$0")/../.." && pwd)"
appid=io.github.it_yohan.Quai
pass=0
ok() { pass=$((pass + 1)); printf '  \033[32m✓\033[0m %s\n' "$*"; }
die() { printf '  \033[31m✗ %s\033[0m\n' "$*"; exit 1; }

mapfile -t rpms < <(find "$dir" -maxdepth 1 -name '*.rpm' ! -name '*.src.rpm' ! -name '*-debuginfo-*' ! -name '*-debugsource-*' | sort)
[[ ${#rpms[@]} -eq 4 ]] || die "expected quai, quai-gui, quai-gtk and quai-kde, found: ${rpms[*]}"
# Tools for the checks and the end-to-end test.
dnf -y -q install desktop-file-utils appstream man-db glibc-langpack-fr rpm-build python3 curl procps-ng >/dev/null
dnf -y -q install "${rpms[@]}" >/dev/null
ok "dnf installs $(printf '%s ' "${rpms[@]##*/}")"

[[ "$(rpm -q --qf '%{version}' quai)" == "$(rpm -q --qf '%{version}' quai-gui)" ]] || die "versions differ"
quai --version | grep -q "^quai " || die "quai --version"
[[ -x /usr/libexec/quaid ]] || die "quaid missing"
getent passwd quai >/dev/null || die "the quai user was not created from sysusers.d"
[[ "$(stat -c '%a %G' /etc/quai/private)" == "750 quai" ]] || die "/etc/quai/private: $(stat -c '%a %G' /etc/quai/private)"
for f in /usr/lib/systemd/system/quaid.socket /usr/lib/systemd/system/quaid.service \
  /usr/share/polkit-1/actions/io.github.it-yohan.quai.policy /usr/share/bash-completion/completions/quai \
  /usr/share/zsh/site-functions/_quai /usr/share/fish/vendor_completions.d/quai.fish; do
  [[ -f "$f" ]] || die "missing $f"
done
ok "quai $(rpm -q --qf '%{version}-%{release}' quai): command, service, quai user, polkit action, completions"

for p in quai quai-gtk quai-kde; do
  n=$(rpm -q --provides "$p" | grep -c '^bundled(crate(' || true)
  [[ $n -gt 20 ]] || die "$p declares $n bundled crates"
  [[ -s /usr/share/licenses/$p/LICENSE.dependencies ]] || die "$p: no LICENSE.dependencies"
done
rpm -q --provides quai-kde | grep -q '^bundled(crate(cxx-qt))' || die "quai-kde should bundle cxx-qt"
rpm -q --provides quai | grep -q '^bundled(crate(gtk4))' && die "quai should not bundle gtk4 (it is quai-gtk's)"
ok "bundled(crate()) Provides and licence lists per binary package"

desktop-file-validate /usr/share/applications/$appid.desktop
appstreamcli validate --no-net --explain /usr/share/metainfo/$appid.metainfo.xml >/dev/null
grep -q '^Comment\[fr\]=' /usr/share/applications/$appid.desktop || die "desktop file not translated"
grep -q 'xml:lang="fr"' /usr/share/metainfo/$appid.metainfo.xml || die "AppStream file not translated"
ok "desktop file and AppStream metadata valid and translated"

[[ -f /usr/share/locale/fr/LC_MESSAGES/quai.mo ]] || die "no French catalogue"
LC_ALL=fr_FR.UTF-8 quai --help | grep -q '^Utilisation' || die "--help is not translated"
LC_ALL=C man -w quai-add >/dev/null || die "no man page for quai-add"
ok "French catalogue, man pages"

echo "== end-to-end test with the installed binaries"
QUAI=/usr/bin/quai QUAID=/usr/libexec/quaid QUAI_KDE=/usr/bin/quai-kde QUAI_LOCALEDIR=/usr/share/locale \
  QUAI_E2E_KDE=system "$top/tests/e2e/run.sh"
ok "end-to-end test passed"
echo
echo "Installed-package checks: $pass passed."
