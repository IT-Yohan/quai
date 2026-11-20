#!/usr/bin/env python3
"""For each repository in repos.txt: RPM assets in its recent releases, the
channels its README offers (dnf repository, COPR, Flathub), and whether
Fedora packages it. Writes results.json. Runs on GitHub Actions."""
import base64, json, os, re, sys, time, urllib.request, urllib.error, concurrent.futures as cf

TOKEN = os.environ["GITHUB_TOKEN"]
repos = [l.split()[0] for l in open(sys.argv[1]) if l.strip()]
out_path = sys.argv[2]

def get(url, auth=False, data=None, accept="application/vnd.github+json"):
    req = urllib.request.Request(url, data=data, headers={"User-Agent": "quai-catalog-scan", "Accept": accept})
    if auth:
        req.add_header("Authorization", f"Bearer {TOKEN}")
    if data is not None:
        req.add_header("Content-Type", "application/json")
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return r.status, r.read()
        except urllib.error.HTTPError as e:
            if e.code in (403, 429) and attempt < 3:
                time.sleep(30 * (attempt + 1)); continue
            return e.code, e.read()
        except Exception as e:  # noqa: BLE001
            if attempt < 3:
                time.sleep(5); continue
            return 0, str(e).encode()

RPM_NAME = re.compile(r"^(?P<name>.+?)[-_]v?\d[\w.~^+]*(?:[-_][\w.~^+]+)?[._-](?P<arch>x86_64|aarch64|amd64|arm64|noarch|i686|ppc64le|s390x|armv7hl|armhfp|riscv64)\.rpm$")
REPO_PAT = {
    "copr": r"copr(\s+enable|\.fedorainfracloud)",
    "repo-file": r"yum\.repos\.d|\.repo\b|dnf\s+config-manager|addrepo|gpgkey\s*=",
    "hosted-repo": r"packagecloud|cloudsmith|fury\.io|rpm\.[a-z0-9.-]+\.(?:com|io|org|dev)",
}

def scan(full):
    o = {"repo": full}
    st, body = get(f"https://api.github.com/repos/{full}/releases?per_page=10", auth=True)
    rels = json.loads(body) if st == 200 else []
    if st != 200:
        o["error"] = f"releases HTTP {st}"
    rels = [r for r in rels if isinstance(r, dict) and not r.get("draft")]
    stable = [r for r in rels if not r.get("prerelease")]
    def rpms(r):
        return [a["name"] for a in r.get("assets", []) if a["name"].endswith(".rpm") and not a["name"].endswith(".src.rpm")]
    latest = (stable or rels or [None])[0]
    o["latest_tag"] = latest["tag_name"] if latest else None
    o["latest_date"] = (latest.get("published_at") or "")[:10] if latest else None
    o["latest_rpms"] = rpms(latest) if latest else []
    with_rpm = [r for r in rels if rpms(r)]
    o["rpm_tag"] = with_rpm[0]["tag_name"] if with_rpm else None
    o["rpm_date"] = (with_rpm[0].get("published_at") or "")[:10] if with_rpm else None
    names = rpms(with_rpm[0]) if with_rpm else []
    o["rpms"] = names
    pkgs, arches = set(), set()
    ARCH = {"x86_64": "x86_64", "amd64": "x86_64", "x64": "x86_64", "x86-64": "x86_64", "aarch64": "aarch64",
            "arm64": "aarch64", "noarch": "noarch", "i686": "i686", "i386": "i686", "386": "i686",
            "ppc64le": "ppc64le", "s390x": "s390x", "armv7hl": "armv7", "armhfp": "armv7", "armv7": "armv7",
            "riscv64": "riscv64"}
    for n in names:
        m = RPM_NAME.match(n)
        if m:
            pkgs.add(m["name"])
        for tok in re.split(r"[._-]", n[:-4].lower()):
            if tok in ARCH:
                arches.add(ARCH[tok])
    o["rpm_packages"] = sorted(pkgs)
    o["arches"] = sorted(arches)
    o["per_fedora"] = bool(any(re.search(r"\.fc\d+\.", n) for n in names))
    # README: the channels upstream documents.
    # Raw files, outside the API quota.
    text = ""
    for name in ("README.md", "readme.md", "README.rst", "Readme.md", "README", "README.adoc", "README.markdown"):
        st, body = get(f"https://raw.githubusercontent.com/{full}/HEAD/{name}")
        if st == 200:
            text = body.decode("utf-8", "replace").lower()
            break
    o["readme"] = {k: bool(re.search(p, text)) for k, p in REPO_PAT.items()}
    o["readme"]["flathub"] = "flathub" in text
    o["readme"]["dnf_install"] = bool(re.search(r"\b(?:sudo\s+)?(?:dnf|yum)\s+install\s+(?!\.|https?:|/|~)", text))
    o["readme"]["snap"] = "snapcraft.io" in text or "snap install" in text
    # Fedora: is a package of that name in rawhide?
    fedora = {}
    candidates = set(pkgs) | {full.split("/")[1].lower()}
    for name in sorted(candidates)[:4]:
        st, body = get(f"https://mdapi.fedoraproject.org/rawhide/srcpkg/{name}")
        if st != 200:
            st, body = get(f"https://mdapi.fedoraproject.org/rawhide/pkg/{name}")
        if st == 200:
            try:
                fedora[name] = json.loads(body).get("version")
            except Exception:  # noqa: BLE001
                fedora[name] = "?"
    o["fedora"] = fedora
    return o

results = []
with cf.ThreadPoolExecutor(6) as ex:
    for i, r in enumerate(ex.map(scan, repos)):
        results.append(r)
        if i % 50 == 0:
            print(i, len(repos), flush=True)
json.dump(results, open(out_path, "w"), indent=1)
print("done", len(results))
