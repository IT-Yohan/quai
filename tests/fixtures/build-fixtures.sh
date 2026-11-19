#!/usr/bin/env bash
# Build the small RPMs used by the test-suite. Requires rpmbuild.
# Usage: tests/fixtures/build-fixtures.sh [output-dir]
set -euo pipefail
out="$(realpath "${1:-$(dirname "$0")/rpms}")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out" "$work"/{SPECS,SOURCES,BUILD,RPMS,SRPMS}

cat > "$work/SPECS/hello.spec" <<'SPEC'
Name:           hello
Version:        1.2.3
Release:        1
Summary:        Says hello <loudly> & proudly
License:        MIT
URL:            https://example.org/hello
BuildArch:      noarch
Requires:       bash >= 4.0
Requires(post): coreutils
Provides:       hello-tool = %{version}-%{release}
Obsoletes:      hello-old < 1.0
Conflicts:      hello-legacy
Recommends:     cowsay
Suggests:       figlet

%description
A tiny test package.
Second line with "quotes".

%install
mkdir -p %{buildroot}/usr/bin %{buildroot}/etc %{buildroot}/usr/share/hello
printf '#!/bin/bash\necho hello\n' > %{buildroot}/usr/bin/hello
chmod 755 %{buildroot}/usr/bin/hello
echo greeting=hi > %{buildroot}/etc/hello.conf
echo doc > %{buildroot}/usr/share/hello/README

%post
true

%files
/usr/bin/hello
%config(noreplace) /etc/hello.conf
%dir /usr/share/hello
/usr/share/hello/README
%ghost /var/log/hello.log

%changelog
* Tue Nov 18 2025 Jane Doe <jane@example.org> - 1.2.3-1
- Newest entry
* Mon Nov 17 2025 Jane Doe <jane@example.org> - 1.2.2-1
- Older entry
SPEC

cat > "$work/SPECS/hello-epoch.spec" <<'SPEC'
Name:           hello-epoch
Epoch:          2
Version:        1.0
Release:        3.fc43
Summary:        Package with an epoch
License:        MIT
BuildArch:      x86_64
AutoReqProv:    no

%description
Epoch test.

%install
mkdir -p %{buildroot}/opt/hello-epoch
echo x > %{buildroot}/opt/hello-epoch/data

%files
/opt/hello-epoch
SPEC

cat > "$work/SPECS/bigheader.spec" <<'SPEC'
Name:           bigheader
Version:        0.9
Release:        1
Summary:        Header larger than 64 KiB
License:        MIT
BuildArch:      noarch

%description
Thousands of files to push the header past the first range request.

%install
mkdir -p %{buildroot}/usr/share/bigheader
for i in $(seq 1 600); do
  : > %{buildroot}/usr/share/bigheader/file-with-a-rather-long-name-to-grow-the-header-$i.txt
done

%files
/usr/share/bigheader
SPEC

for spec in hello hello-epoch bigheader; do
  rpmbuild --quiet --define "_topdir $work" --define "_build_id_links none" \
    --define "__brp_compress true" -bb "$work/SPECS/$spec.spec" >/dev/null
done
find "$work/RPMS" -name '*.rpm' -exec cp {} "$out/" \;
ls -l "$out"
