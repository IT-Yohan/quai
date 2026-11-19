#!/usr/bin/env python3
"""Compute the License: tag of each binary subpackage of quai.spec.

Fedora asks the License tag of a Rust binary to cover every crate linked into
it, not only the project's own license. This reads `cargo tree` (normal
dependencies of the Linux target: no build scripts, tests or proc-macros,
which do not end up in the binary) and builds one SPDX expression per
subpackage. The spec keeps them in %global license_* lines.

  packaging/licenses.py           print the expressions
  packaging/licenses.py --write   update packaging/quai.spec
  packaging/licenses.py --check   fail if the spec is out of date (CI)
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPEC = ROOT / "packaging" / "quai.spec"
TARGET = "x86_64-unknown-linux-gnu"
# Subpackage macro -> crates whose binaries it ships.
PACKAGES = {
    "license_core": ["quai-cli", "quaid"],
    "license_gtk": ["quai-gtk"],
    "license_kde": ["quai-kde"],
}


def crate_licenses(crate):
    out = subprocess.run(
        ["cargo", "tree", "-q", "--offline", "-p", crate, "--edges", "no-build,no-dev,no-proc-macro",
         "--target", TARGET, "--prefix", "none", "--format", "{l}"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return {line.replace(" (*)", "").strip() for line in out.splitlines() if line.strip()}


def normalize(expr):
    """One canonical spelling per license choice: legacy "A/B" means "A OR B"."""
    expr = re.sub(r"\s*/\s*", " OR ", expr)
    if " AND " in expr and " OR " in expr:
        return f"({expr})"
    if " OR " in expr:
        return "(" + " OR ".join(sorted(t.strip() for t in expr.split(" OR "))) + ")"
    if " AND " in expr:
        return " AND ".join(sorted(t.strip() for t in expr.split(" AND ")))
    return expr


def expression(crates):
    terms = set()
    for crate in crates:
        for lic in crate_licenses(crate):
            for part in normalize(lic).split(" AND ") if not normalize(lic).startswith("(") else [normalize(lic)]:
                terms.add(part)
    terms.add("Apache-2.0")  # quai itself
    simple = sorted(t for t in terms if not t.startswith("("))
    choices = sorted(t for t in terms if t.startswith("("))
    return " AND ".join(simple + choices)


def main():
    exprs = {name: expression(crates) for name, crates in PACKAGES.items()}
    spec = SPEC.read_text()
    updated = spec
    for name, expr in exprs.items():
        updated = re.sub(rf"^%global {name} .*$", f"%global {name} {expr}", updated, flags=re.M)
    if "--write" in sys.argv:
        SPEC.write_text(updated)
    elif "--check" in sys.argv:
        if updated != spec:
            sys.exit("packaging/quai.spec: License tags are out of date; run packaging/licenses.py --write")
    else:
        for name, expr in exprs.items():
            print(f"{name}: {expr}")


if __name__ == "__main__":
    main()
