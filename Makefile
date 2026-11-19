# quai — build, test and install.
#
#   make                 release build
#   make check           unit + integration tests
#   make e2e             end-to-end test against dnf (root, needs rpmbuild + dnf)
#   sudo make install    install under PREFIX (default /usr)
#   make dist            source and vendored-dependency tarballs for the RPM spec

PREFIX      ?= /usr
BINDIR      ?= $(PREFIX)/bin
LIBEXECDIR  ?= $(PREFIX)/libexec
DATADIR     ?= $(PREFIX)/share
SYSCONFDIR  ?= /etc
UNITDIR     ?= $(PREFIX)/lib/systemd/system
SYSUSERSDIR ?= $(PREFIX)/lib/sysusers.d
DESTDIR     ?=
CARGO       ?= cargo
TARGET_DIR  ?= target/release
VERSION     := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APPID       := io.github.quai.Quai

.PHONY: all build check e2e fixtures lint install uninstall dist clean

all: build

build:
	$(CARGO) build --release --workspace

check:
	$(CARGO) test --workspace

lint:
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fixtures:
	tests/fixtures/build-fixtures.sh

e2e:
	$(CARGO) build --workspace
	tests/e2e/run.sh target/debug

install:
	install -Dpm0755 $(TARGET_DIR)/quai           $(DESTDIR)$(BINDIR)/quai
	install -Dpm0755 $(TARGET_DIR)/quaid          $(DESTDIR)$(LIBEXECDIR)/quaid
	install -Dpm0755 $(TARGET_DIR)/quai-gtk       $(DESTDIR)$(BINDIR)/quai-gtk
	install -Dpm0644 data/systemd/quaid.socket    $(DESTDIR)$(UNITDIR)/quaid.socket
	install -Dpm0644 data/systemd/quaid.service   $(DESTDIR)$(UNITDIR)/quaid.service
	install -Dpm0644 data/sysusers/quai.conf      $(DESTDIR)$(SYSUSERSDIR)/quai.conf
	install -Dpm0644 data/polkit/io.github.quai.policy $(DESTDIR)$(DATADIR)/polkit-1/actions/io.github.quai.policy
	install -Dpm0644 data/desktop/$(APPID).desktop $(DESTDIR)$(DATADIR)/applications/$(APPID).desktop
	install -Dpm0644 data/metainfo/$(APPID).metainfo.xml $(DESTDIR)$(DATADIR)/metainfo/$(APPID).metainfo.xml
	install -Dpm0644 data/icons/hicolor/scalable/apps/$(APPID).svg $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/$(APPID).svg
	install -Dpm0644 data/icons/hicolor/symbolic/apps/$(APPID)-symbolic.svg $(DESTDIR)$(DATADIR)/icons/hicolor/symbolic/apps/$(APPID)-symbolic.svg
	install -dm0755 $(DESTDIR)$(SYSCONFDIR)/quai/sources.d
	test -e $(DESTDIR)$(SYSCONFDIR)/quai/quai.toml || install -Dpm0644 data/quai.toml $(DESTDIR)$(SYSCONFDIR)/quai/quai.toml
	@echo
	@echo "Installed. On a live system, finish with:"
	@echo "  sudo systemd-sysusers && sudo systemctl daemon-reload && sudo quai setup"

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/quai $(DESTDIR)$(LIBEXECDIR)/quaid $(DESTDIR)$(BINDIR)/quai-gtk
	rm -f $(DESTDIR)$(UNITDIR)/quaid.socket $(DESTDIR)$(UNITDIR)/quaid.service
	rm -f $(DESTDIR)$(SYSUSERSDIR)/quai.conf $(DESTDIR)$(DATADIR)/polkit-1/actions/io.github.quai.policy
	rm -f $(DESTDIR)$(DATADIR)/applications/$(APPID).desktop $(DESTDIR)$(DATADIR)/metainfo/$(APPID).metainfo.xml
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/$(APPID).svg
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/symbolic/apps/$(APPID)-symbolic.svg
	@echo "Configuration in $(SYSCONFDIR)/quai and $(SYSCONFDIR)/yum.repos.d/quai-*.repo was kept."

dist:
	git archive --format=tar.gz --prefix=quai-$(VERSION)/ -o quai-$(VERSION).tar.gz HEAD
	rm -rf vendor && $(CARGO) vendor --locked --versioned-dirs vendor >/dev/null
	tar -cJf quai-$(VERSION)-vendor.tar.xz vendor && rm -rf vendor
	@ls -l quai-$(VERSION).tar.gz quai-$(VERSION)-vendor.tar.xz

clean:
	$(CARGO) clean
