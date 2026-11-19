# quai — build, test and install.
#
#   make                 release build, translations, man pages, completions
#   make check           unit + integration tests
#   make e2e             end-to-end test against dnf (root, needs rpmbuild + dnf)
#   make kde-ui-test     drive the Kirigami frontend (needs a running e2e tree)
#   sudo make install    install under PREFIX (default /usr)
#   make dist            source and vendored-dependency tarballs for the RPM spec
#
# Translations (gettext, one catalogue for every program and data file):
#   make pot             regenerate po/quai.pot from the sources
#   make update-po       merge po/quai.pot into every po/<lang>.po
#   make check-po        fail if po/quai.pot is stale or a catalogue is invalid
#
# FRONTENDS selects the graphical frontends (default: both). Without Qt 6, use
# `make FRONTENDS=gtk`; for a headless machine, `make FRONTENDS=`.

PREFIX      ?= /usr
BINDIR      ?= $(PREFIX)/bin
LIBEXECDIR  ?= $(PREFIX)/libexec
DATADIR     ?= $(PREFIX)/share
MANDIR      ?= $(DATADIR)/man
LOCALEDIR   ?= $(DATADIR)/locale
SYSCONFDIR  ?= /etc
UNITDIR     ?= $(PREFIX)/lib/systemd/system
SYSUSERSDIR ?= $(PREFIX)/lib/sysusers.d
BASHCOMPDIR ?= $(DATADIR)/bash-completion/completions
ZSHCOMPDIR  ?= $(DATADIR)/zsh/site-functions
FISHCOMPDIR ?= $(DATADIR)/fish/vendor_completions.d
DESTDIR     ?=
CARGO       ?= cargo
TARGET_DIR  ?= target/release
GEN         ?= target/generated
VERSION     := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APPID       := io.github.quai.Quai
POLICY      := io.github.quai.policy
FRONTENDS   ?= gtk kde
PACKAGES    := -p quai-i18n -p quai-types -p quai-core -p quai-cli -p quaid $(foreach f,$(FRONTENDS),-p quai-$(f))
LINGUAS     := $(shell sed 's/#.*//' po/LINGUAS)
# quai-i18n itself only defines the functions (and tests them with sample messages).
SOURCES_I18N = $(sort $(filter-out crates/quai-i18n/%,$(wildcard crates/*/src/*.rs crates/*/src/*/*.rs)) $(wildcard crates/quai-kde/qml/*.qml))
DATA_IN     := data/desktop/$(APPID).desktop.in data/metainfo/$(APPID).metainfo.xml.in data/polkit/$(POLICY).in
# Our ITS rules for polkit files, next to gettext's own (metainfo, desktop).
export GETTEXTDATADIRS := $(CURDIR)/po

.PHONY: all build translations docs check e2e kde-ui-test fixtures lint pot update-po check-po \
	install install-core install-gui install-translations uninstall dist clean $(addprefix install-,$(FRONTENDS))

all: build

build:
	QUAI_LOCALEDIR=$(LOCALEDIR) $(CARGO) build --release $(PACKAGES)
	$(MAKE) translations docs

# Compiled catalogues and translated data files, under $(GEN).
translations:
	@for lang in $(LINGUAS); do \
		mkdir -p $(GEN)/locale/$$lang/LC_MESSAGES && \
		msgfmt --check -o $(GEN)/locale/$$lang/LC_MESSAGES/quai.mo po/$$lang.po || exit 1; \
	done
	mkdir -p $(GEN)
	msgfmt --desktop -d po --template data/desktop/$(APPID).desktop.in -o $(GEN)/$(APPID).desktop
	msgfmt --xml -d po --template data/metainfo/$(APPID).metainfo.xml.in -o $(GEN)/$(APPID).metainfo.xml
	msgfmt --xml -d po --template data/polkit/$(POLICY).in -o $(GEN)/$(POLICY)

# Man pages and shell completions, from quai's command line definition
# (quai-generate is a build tool of the quai-cli crate; it is not installed).
docs:
	rm -rf $(GEN)/man $(GEN)/completions
	$(TARGET_DIR)/quai-generate man $(GEN)/man
	$(TARGET_DIR)/quai-generate completions $(GEN)/completions

check:
	$(CARGO) test $(PACKAGES)

lint:
	$(CARGO) fmt --all --check
	$(CARGO) clippy $(PACKAGES) --all-targets -- -D warnings

fixtures:
	tests/fixtures/build-fixtures.sh

e2e:
	$(CARGO) build $(PACKAGES)
	$(MAKE) translations
	tests/e2e/run.sh target/debug

kde-ui-test:
	$(CARGO) build -p quai-kde
	crates/quai-kde/tests/kde-ui-test.sh target/debug/quai-kde

# po/quai.pot: messages of the Rust and QML code, of `quai --help`, and of the
# desktop, AppStream and polkit files. Locations name files only, so the
# template changes when messages do, not when lines move.
pot:
	$(CARGO) build -q -p quai-cli
	mkdir -p $(GEN)/pot
	po/extract.py $(GEN)/pot/code.pot $(SOURCES_I18N)
	target/debug/quai-generate pot $(GEN)/pot/help.pot
	xgettext --from-code=UTF-8 --add-comments=Translators: --omit-header -o $(GEN)/pot/data.pot $(DATA_IN)
	msgcat --use-first --add-location=file -o po/quai.pot $(GEN)/pot/code.pot $(GEN)/pot/help.pot $(GEN)/pot/data.pot
	@msgattrib --no-obsolete po/quai.pot | grep -c '^msgid ' | xargs -I{} echo "po/quai.pot: {} messages (header included)"

update-po: pot
	@for lang in $(LINGUAS); do \
		msgmerge --quiet --update --backup=none --previous --add-location=file po/$$lang.po po/quai.pot || exit 1; \
		printf '%s: ' $$lang; msgfmt --statistics -o /dev/null po/$$lang.po; \
	done

check-po:
	cp po/quai.pot $(GEN)/quai.pot.committed 2>/dev/null || { mkdir -p $(GEN) && cp po/quai.pot $(GEN)/quai.pot.committed; }
	$(MAKE) -s pot
	@diff -u $(GEN)/quai.pot.committed po/quai.pot >/dev/null || { \
		cp $(GEN)/quai.pot.committed po/quai.pot; \
		echo "po/quai.pot is out of date: run 'make pot' (and 'make update-po') and commit the result"; exit 1; }
	@for lang in $(LINGUAS); do \
		msgfmt --check --check-format -o /dev/null po/$$lang.po || exit 1; \
		printf '%s: ' $$lang; msgfmt --statistics -o /dev/null po/$$lang.po; \
	done

install: install-core install-translations $(if $(strip $(FRONTENDS)),install-gui) $(addprefix install-,$(FRONTENDS))
	@echo
	@echo "Installed. On a live system, finish with:"
	@echo "  sudo systemd-sysusers && sudo systemctl daemon-reload && sudo quai setup"

install-gtk:
	install -Dpm0755 $(TARGET_DIR)/quai-gtk       $(DESTDIR)$(BINDIR)/quai-gtk

install-kde:
	install -Dpm0755 $(TARGET_DIR)/quai-kde       $(DESTDIR)$(BINDIR)/quai-kde

install-gui:
	install -Dpm0644 $(GEN)/$(APPID).desktop $(DESTDIR)$(DATADIR)/applications/$(APPID).desktop
	install -Dpm0644 $(GEN)/$(APPID).metainfo.xml $(DESTDIR)$(DATADIR)/metainfo/$(APPID).metainfo.xml
	install -Dpm0644 data/icons/hicolor/scalable/apps/$(APPID).svg $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/$(APPID).svg
	install -Dpm0644 data/icons/hicolor/symbolic/apps/$(APPID)-symbolic.svg $(DESTDIR)$(DATADIR)/icons/hicolor/symbolic/apps/$(APPID)-symbolic.svg

install-translations:
	@for lang in $(LINGUAS); do \
		install -Dpm0644 $(GEN)/locale/$$lang/LC_MESSAGES/quai.mo $(DESTDIR)$(LOCALEDIR)/$$lang/LC_MESSAGES/quai.mo || exit 1; \
	done

install-core:
	install -Dpm0755 $(TARGET_DIR)/quai           $(DESTDIR)$(BINDIR)/quai
	install -Dpm0755 $(TARGET_DIR)/quaid          $(DESTDIR)$(LIBEXECDIR)/quaid
	install -Dpm0644 data/systemd/quaid.socket    $(DESTDIR)$(UNITDIR)/quaid.socket
	install -Dpm0644 data/systemd/quaid.service   $(DESTDIR)$(UNITDIR)/quaid.service
	install -Dpm0644 data/sysusers/quai.conf      $(DESTDIR)$(SYSUSERSDIR)/quai.conf
	install -Dpm0644 $(GEN)/$(POLICY)             $(DESTDIR)$(DATADIR)/polkit-1/actions/$(POLICY)
	install -dm0755 $(DESTDIR)$(MANDIR)/man1
	install -pm0644 $(GEN)/man/*.1                $(DESTDIR)$(MANDIR)/man1/
	install -Dpm0644 $(GEN)/completions/quai.bash $(DESTDIR)$(BASHCOMPDIR)/quai
	install -Dpm0644 $(GEN)/completions/_quai     $(DESTDIR)$(ZSHCOMPDIR)/_quai
	install -Dpm0644 $(GEN)/completions/quai.fish $(DESTDIR)$(FISHCOMPDIR)/quai.fish
	install -dm0755 $(DESTDIR)$(SYSCONFDIR)/quai/sources.d
	test -e $(DESTDIR)$(SYSCONFDIR)/quai/quai.toml || install -Dpm0644 data/quai.toml $(DESTDIR)$(SYSCONFDIR)/quai/quai.toml

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/quai $(DESTDIR)$(LIBEXECDIR)/quaid $(DESTDIR)$(BINDIR)/quai-gtk $(DESTDIR)$(BINDIR)/quai-kde
	rm -f $(DESTDIR)$(UNITDIR)/quaid.socket $(DESTDIR)$(UNITDIR)/quaid.service
	rm -f $(DESTDIR)$(SYSUSERSDIR)/quai.conf $(DESTDIR)$(DATADIR)/polkit-1/actions/$(POLICY)
	rm -f $(DESTDIR)$(DATADIR)/applications/$(APPID).desktop $(DESTDIR)$(DATADIR)/metainfo/$(APPID).metainfo.xml
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/$(APPID).svg
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/symbolic/apps/$(APPID)-symbolic.svg
	rm -f $(DESTDIR)$(MANDIR)/man1/quai.1 $(DESTDIR)$(MANDIR)/man1/quai-*.1
	rm -f $(DESTDIR)$(BASHCOMPDIR)/quai $(DESTDIR)$(ZSHCOMPDIR)/_quai $(DESTDIR)$(FISHCOMPDIR)/quai.fish
	rm -f $(DESTDIR)$(LOCALEDIR)/*/LC_MESSAGES/quai.mo
	@echo "Configuration in $(SYSCONFDIR)/quai and $(SYSCONFDIR)/yum.repos.d/quai-*.repo was kept."

dist:
	git archive --format=tar.gz --prefix=quai-$(VERSION)/ -o quai-$(VERSION).tar.gz HEAD
	rm -rf vendor && $(CARGO) vendor --locked --versioned-dirs vendor >/dev/null
	tar -cJf quai-$(VERSION)-vendor.tar.xz vendor && rm -rf vendor
	@ls -l quai-$(VERSION).tar.gz quai-$(VERSION)-vendor.tar.xz

clean:
	$(CARGO) clean
