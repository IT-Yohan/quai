# COPR's "make srpm" build method (Packit can use it too). COPR runs, as root
# in a Fedora chroot with network access:
#   make -f .copr/Makefile srpm outdir=<dir> spec=<spec>
# The release workflow submits source RPMs itself; this is for builds that
# COPR starts from a webhook or by hand ("Rebuild" in the web interface).
outdir ?= $(CURDIR)/dist

srpm:
	dnf -y install git-core cargo rpm-build python3 xz cargo-vendor-filterer
	git config --global --add safe.directory "$(CURDIR)"
	packaging/make-srpm.sh --outdir "$(outdir)"

.PHONY: srpm
