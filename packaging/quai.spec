# RPM spec for quai, suitable for COPR. Build the two sources with `make dist`.
%global appid io.github.quai.Quai

Name:           quai
Version:        0.1.0
Release:        1%{?dist}
Summary:        Keep RPMs published as GitHub, GitLab or Forgejo releases updated with dnf

# quai is Apache-2.0; the vendored crates carry their own licenses, summarised
# in LICENSE.dependencies at build time (%%cargo_license_summary).
License:        Apache-2.0
URL:            https://github.com/quai-rpm/quai
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz

ExclusiveArch:  %{rust_arches}

BuildRequires:  cargo-rpm-macros >= 24
BuildRequires:  gcc-c++
BuildRequires:  pkgconfig(gtk4) >= 4.14
BuildRequires:  pkgconfig(libadwaita-1) >= 1.5
# quai-kde (cxx-qt finds Qt through qmake6)
BuildRequires:  qt6-qtbase-devel >= 6.6
BuildRequires:  qt6-qtdeclarative-devel >= 6.6
BuildRequires:  systemd-rpm-macros
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib
# Used by the test-suite to cross-check the RPM header reader.
BuildRequires:  rpm-build
BuildRequires:  createrepo_c

Requires:       (dnf5 or dnf or rpm-ostree)
Requires:       systemd
# The quai user and group come from %%{_sysusersdir}/quai.conf (rpm >= 4.19 creates them).
# On a desktop, bring the window too; quai-gui picks the frontend that fits it.
Recommends:     (%{name}-gui if (gnome-shell or plasma-workspace))

%description
quai turns projects that attach .rpm files to their GitHub, GitLab or
Forgejo releases into small signed dnf repositories served on 127.0.0.1 by a
socket-activated service. dnf, GNOME Software and rpm-ostree then install,
update and roll back those packages like any other Fedora package.

This package contains the quai command and the quaid service.

%package gui
Summary:        Graphical interface for quai, native on GNOME and KDE Plasma
BuildArch:      noarch
Requires:       %{name} = %{version}-%{release}
Requires:       (%{name}-gtk or %{name}-kde)
# Install the frontend that matches the desktop (both with two desktops; GTK
# on any other one). Checked by tests/packaging/weak-deps.sh.
Recommends:     (%{name}-kde if plasma-workspace else %{name}-gtk)
Recommends:     (%{name}-gtk if gnome-shell)

%description gui
The "Quai" application: one launcher, menu entry and AppStream component.
It runs `quai gui`, which opens the Kirigami frontend on KDE Plasma and the
GTK/libadwaita frontend elsewhere, falling back to whichever is installed.

%package gtk
Summary:        GTK 4 / libadwaita frontend for quai
Requires:       %{name}%{?_isa} = %{version}-%{release}
Requires:       %{name}-gui = %{version}-%{release}
Requires:       polkit

%description gtk
A GTK 4 / libadwaita window to add, inspect and remove quai sources, native
on GNOME. Changes go through polkit; updates themselves are applied by dnf or
GNOME Software.

%package kde
Summary:        Kirigami frontend for quai
Requires:       %{name}%{?_isa} = %{version}-%{release}
Requires:       %{name}-gui = %{version}-%{release}
Requires:       polkit
Requires:       kf6-kirigami%{?_isa}
Requires:       kf6-qqc2-desktop-style%{?_isa}

%description kde
A Kirigami window to add, inspect and remove quai sources, native on KDE
Plasma. Changes go through polkit; updates themselves are applied by dnf or
Discover.

%prep
%autosetup -n %{name}-%{version} -p1 -a1
%cargo_prep -v vendor

%build
export QMAKE=%{?_qt6_qmake}%{!?_qt6_qmake:qmake6}
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%cargo_vendor_manifest

%install
%make_install TARGET_DIR=target/rpm PREFIX=%{_prefix} LIBEXECDIR=%{_libexecdir} \
    UNITDIR=%{_unitdir} SYSUSERSDIR=%{_sysusersdir} SYSCONFDIR=%{_sysconfdir}
install -dm0750 %{buildroot}%{_sysconfdir}/quai/private

%check
export QMAKE=%{?_qt6_qmake}%{!?_qt6_qmake:qmake6}
%cargo_test
desktop-file-validate %{buildroot}%{_datadir}/applications/%{appid}.desktop
appstream-util validate-relax --nonet %{buildroot}%{_metainfodir}/%{appid}.metainfo.xml

%post
%systemd_post quaid.socket

%preun
%systemd_preun quaid.socket quaid.service

%postun
%systemd_postun_with_restart quaid.service
if [ $1 -eq 0 ]; then
    # Package removed: drop the .repo files quai generated so dnf does not
    # keep pointing at a service that no longer exists.
    for f in %{_sysconfdir}/yum.repos.d/quai-*.repo; do
        [ -f "$f" ] && head -n1 "$f" | grep -q '^# Managed by quai' && rm -f "$f"
    done
fi
:

%files
%license LICENSE LICENSE.dependencies cargo-vendor.txt
%doc README.md
%{_bindir}/quai
%{_libexecdir}/quaid
%{_unitdir}/quaid.socket
%{_unitdir}/quaid.service
%{_sysusersdir}/quai.conf
%{_datadir}/polkit-1/actions/io.github.quai.policy
%dir %{_sysconfdir}/quai
%dir %{_sysconfdir}/quai/sources.d
%attr(0750,root,quai) %dir %{_sysconfdir}/quai/private
%config(noreplace) %{_sysconfdir}/quai/quai.toml

%files gui
%{_datadir}/applications/%{appid}.desktop
%{_metainfodir}/%{appid}.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/%{appid}.svg
%{_datadir}/icons/hicolor/symbolic/apps/%{appid}-symbolic.svg

%files gtk
%{_bindir}/quai-gtk

%files kde
%{_bindir}/quai-kde

%changelog
* Thu Nov 19 2026 The quai contributors - 0.1.0-1
- First release
