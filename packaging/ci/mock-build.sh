#!/usr/bin/env bash
# Rebuild a source RPM with mock, the way COPR and Koji build it: a clean
# buildroot with only the BuildRequires, no network. Meant for a privileged
# Fedora container (CI); the binary RPMs and logs end up in OUTDIR.
#
#   packaging/ci/mock-build.sh path/to/quai.src.rpm <fedora release|rawhide> OUTDIR
set -euo pipefail
srpm="$(realpath "$1")"
release="$2"
out="$(realpath -m "$3")"
arch="$(uname -m)"
config="fedora-$release-$arch"

dnf -y -q install mock
# mock drops privileges for rpmbuild: run it as a member of the mock group.
id mockbuilder >/dev/null 2>&1 || useradd -m mockbuilder
usermod -a -G mock mockbuilder
# Inside Docker mock cannot unshare namespaces; the container is the sandbox.
grep -q docker_unshare_warning /etc/mock/site-defaults.cfg \
  || echo "config_opts['docker_unshare_warning'] = False" >> /etc/mock/site-defaults.cfg

work="$(mktemp -d /tmp/mock-build.XXXXXX)"
cp "$srpm" "$work/"
mkdir -p "$out"
chown -R mockbuilder "$work" "$out"
echo "== mock -r $config --rebuild $(basename "$srpm")"
status=0
su mockbuilder -s /bin/bash -c \
  "mock -r '$config' --isolation=simple --no-bootstrap-image --resultdir '$out' --rebuild '$work/$(basename "$srpm")'" \
  || status=$?
if [[ $status -ne 0 ]]; then
  for log in root.log build.log; do
    [[ -f "$out/$log" ]] && { echo "== tail of $log"; tail -n 80 "$out/$log"; }
  done
  exit "$status"
fi
ls -l "$out"/*.rpm
