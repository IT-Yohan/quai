#!/usr/bin/env python3
"""Check that a tag is ready to release, and print its notes in Markdown.

The release notes live in one place, the <releases> of the AppStream file,
which GNOME Software and Discover show as "What's new". This checks that the
tag, Cargo.toml, the spec's %changelog and the AppStream file agree, then
turns that <release> into the GitHub release text.

    packaging/release-notes.py v0.1.0 > notes.md
"""

import re
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
METAINFO = ROOT / "data/metainfo/io.github.it_yohan.Quai.metainfo.xml.in"
SPEC = ROOT / "packaging/quai.spec"


def fail(msg):
    sys.exit(f"release-notes: {msg}")


def text(el):
    """Inline text of an element, with <code> and <em> as Markdown."""
    out = el.text or ""
    for child in el:
        inner = text(child)
        out += {"code": f"`{inner}`", "em": f"*{inner}*"}.get(child.tag, inner)
        out += child.tail or ""
    return " ".join(out.split())


def markdown(description):
    blocks = []
    for el in description:
        if el.tag == "p":
            blocks.append(text(el))
        elif el.tag in ("ul", "ol"):
            items = [text(li) for li in el if li.tag == "li"]
            bullet = (lambda i: f"{i + 1}.") if el.tag == "ol" else (lambda i: "-")
            blocks.append("\n".join(f"{bullet(i)} {item}" for i, item in enumerate(items)))
    return "\n\n".join(blocks)


def main():
    if len(sys.argv) != 2:
        fail("usage: release-notes.py vX.Y.Z")
    tag = sys.argv[1]
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:[-~][0-9A-Za-z.]+)?", tag):
        fail(f"{tag} is not a version tag (vX.Y.Z)")
    version = tag[1:]

    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    if cargo != version:
        fail(f"Cargo.toml says {cargo}, the tag says {version}")

    spec = SPEC.read_text()
    changelog = spec.split("%changelog", 1)[1].strip().splitlines()
    if not changelog or not re.search(rf" - {re.escape(version)}-\d+\s*$", changelog[0]):
        fail(f"the first %changelog entry of packaging/quai.spec is not for {version}: {changelog[:1]}")

    releases = ET.parse(METAINFO).getroot().find("releases")
    first = releases[0] if releases is not None and len(releases) else None
    if first is None or first.get("version") != version:
        fail(f"the first <release> of {METAINFO.name} is not {version}")
    if not first.get("date"):
        fail(f"<release version=\"{version}\"> has no date")
    description = first.find("description")
    if description is None or not len(description):
        fail(f"<release version=\"{version}\"> has no description")

    print(markdown(description))


if __name__ == "__main__":
    main()
