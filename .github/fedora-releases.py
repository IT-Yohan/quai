#!/usr/bin/env python3
"""Print the Fedora releases quai is built for, as GitHub Actions outputs.

The list comes from Bodhi (Fedora's update system): every release in the
"current" state, so a new Fedora joins the matrix the day it ships and an
end-of-life one leaves it. If Bodhi cannot be reached, a fixed list is used
and the run says so.

    fedora=["44", "45"]        stable releases (tests, packages, GitHub release)
    newest=45                  the release that builds the source RPM
"""

import json
import os
import sys
import urllib.request

FALLBACK = ["44", "45"]
URL = "https://bodhi.fedoraproject.org/releases/?state=current&rows_per_page=100"


def current_releases():
    req = urllib.request.Request(URL, headers={"Accept": "application/json", "User-Agent": "quai-ci"})
    with urllib.request.urlopen(req, timeout=30) as r:
        data = json.load(r)
    versions = {
        rel["version"]
        for rel in data.get("releases", [])
        # Plain Fedora: not EPEL, ELN, containers or Flatpaks.
        if rel.get("id_prefix") == "FEDORA" and rel.get("version", "").isdigit()
    }
    return sorted(versions, key=int)


def main():
    try:
        releases = current_releases()
        if not releases:
            raise ValueError("no current Fedora release in Bodhi's answer")
    except Exception as e:  # noqa: BLE001 - any failure falls back
        print(f"::warning::Bodhi unavailable ({e}); building for Fedora {', '.join(FALLBACK)}")
        releases = FALLBACK
    lines = [f"fedora={json.dumps(releases)}", f"newest={releases[-1]}"]
    out = os.environ.get("GITHUB_OUTPUT")
    if out:
        with open(out, "a", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    sys.exit(main())
