# Releasing Quai

A release is a tag. Pushing `vX.Y.Z` runs [`release.yml`](../.github/workflows/release.yml),
which publishes the GitHub release and builds the same source RPM in COPR.

## Each release

1. Bump `version` in `Cargo.toml` (`cargo update -w` refreshes `Cargo.lock`).
2. Describe the release in `data/metainfo/io.github.it_yohan.Quai.metainfo.xml.in`: a new
   first `<release version="X.Y.Z" date="YYYY-MM-DD">` with a `<description>`. GNOME
   Software and Discover show it as "What's new"; the GitHub release text is made from it.
3. Add the matching first entry to `%changelog` in `packaging/quai.spec`
   (`* Day Mon DD YYYY Name <email> - X.Y.Z-1`).
4. `make update-po`, then translate the new release notes in `po/*.po`.
5. If the windows changed, run the **Screenshots** workflow, then bring its images in with
   `git checkout origin/screenshots -- data/screenshots`: the AppStream file points at them
   under the release tag.
6. Commit, wait for CI to pass on `main`, then either push a tag:

   ```
   git tag -a vX.Y.Z -m "Quai X.Y.Z"
   git push origin vX.Y.Z
   ```

   or, without git (the GitHub mobile app works too): Actions → Release → Run workflow, branch
   `main`, tag `vX.Y.Z`. The workflow creates the tag once the checks below pass.

`packaging/release-notes.py vX.Y.Z` runs first and stops the release if the tag,
`Cargo.toml`, the spec and the AppStream file disagree. "Run workflow" on the Release
workflow redoes a release for an existing tag; `copr_only` resubmits only the COPR build
(for instance after setting up COPR, or when its token had expired).

## What gets published

| Where | What |
|---|---|
| GitHub release | `quai`, `quai-gui`, `quai-gtk`, `quai-kde` for each supported Fedora release (`.fcNN`), x86_64 and aarch64; the source RPM; `quai-X.Y.Z-vendor.tar.xz` (the spec's `Source1`); `SHA256SUMS` |
| COPR | the same source RPM, built for every Fedora release and rawhide, with AppStream metadata so GNOME Software and Discover list Quai |

The supported Fedora releases come from Bodhi when the workflow runs, so a new Fedora is
built for from its release day. COPR adds new branches and removes end-of-life ones by itself
(`--follow-fedora-branching`).

Snapshots: every push builds and tests the packages (`ci.yml`) with a version such as
`0.1.0^20261120git1a2b3c4`; they are kept as workflow artifacts, not published.

## One-time COPR setup

1. Create a Fedora account at <https://accounts.fedoraproject.org> (the COPR owner name is its
   user name) and log in once at <https://copr.fedorainfracloud.org>.
2. Get an API token at <https://copr.fedorainfracloud.org/api/>. It is a small INI file
   starting with `[copr-cli]`.
3. In the GitHub repository: Settings → Secrets and variables → Actions → New repository
   secret, name `COPR_CONFIG`, value: the whole file.
4. Optional: a repository variable `COPR_PROJECT` to use another project name than `quai`
   (the README points at `it-yohan/quai`).

The first release (or "Run workflow" with `copr_only`) creates the project with the
description and instructions in `packaging/copr/`, and updates them on every release.
COPR tokens expire after 180 days; the COPR job then fails with a message saying so, and a
new token in the secret is all it takes.
