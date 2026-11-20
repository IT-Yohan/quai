#!/usr/bin/env python3
"""Which of the graphical apps that only ship RPMs in their releases are the
most used, and does quai serve them on Fedora?

For each repository in gui.txt:
  * RPM download counts over its last 10 releases (x86_64 and in total), from
    the GitHub API: the best usage signal there is for Fedora users;
  * `quai --json preview owner/repo`, from the quai RPM installed from COPR, on
    this Fedora container: what quai would actually serve here.
Then, for the TOP most downloaded that quai serves, download the x86_64 RPM and
run a dnf test transaction: do its dependencies resolve on this Fedora?

Writes usage.json. Runs on GitHub Actions, in a fedora container, as root."""
import datetime, json, os, subprocess, sys, time, urllib.error, urllib.request
import concurrent.futures as cf

TOKEN = os.environ["GITHUB_TOKEN"]
# One line per entry: owner/repo[#label] [asset-pattern]
entries = [l.split() for l in open(sys.argv[1]) if l.strip()]
repos = [e[0] for e in entries]
patterns = {e[0]: e[1] for e in entries if len(e) > 1}
out_path = sys.argv[2]
TOP = int(os.environ.get("TOP", "40"))
NOW = datetime.datetime.now(datetime.timezone.utc)


def get(url):
    req = urllib.request.Request(url, headers={
        "User-Agent": "quai-catalog-scan", "Accept": "application/vnd.github+json",
        "Authorization": f"Bearer {TOKEN}"})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return json.load(r)
        except urllib.error.HTTPError as e:
            if e.code in (403, 429) and attempt < 3:
                time.sleep(60 * (attempt + 1)); continue
            return None
        except Exception:  # noqa: BLE001
            time.sleep(5)
    return None


def is_rpm(name):
    return name.endswith(".rpm") and not name.endswith(".src.rpm")


def is_x86(name):
    return any(a in name for a in ("x86_64", "amd64", "x64", "noarch"))


def stats(repo):
    repo = repo.split("#")[0]
    rels = get(f"https://api.github.com/repos/{repo}/releases?per_page=10") or []
    o = {"rpm": 0, "rpm_x86": 0, "releases": 0, "oldest": None, "urls": {}, "latest_assets": None}
    for r in rels:
        if r.get("draft"):
            continue
        rpms = [a for a in r["assets"] if is_rpm(a["name"])]
        if not rpms:
            continue
        o["releases"] += 1
        o["rpm"] += sum(a["download_count"] for a in rpms)
        o["rpm_x86"] += sum(a["download_count"] for a in rpms if is_x86(a["name"]))
        o["oldest"] = r["published_at"]
        if o["latest_assets"] is None:
            o["latest_assets"] = {a["name"]: a["download_count"] for a in rpms}
        for a in rpms:
            o["urls"].setdefault(a["name"], a["browser_download_url"])
    if o["oldest"]:
        d = datetime.datetime.fromisoformat(o["oldest"].replace("Z", "+00:00"))
        o["days"] = max((NOW - d).days, 1)
        o["x86_per_day"] = round(o["rpm_x86"] / o["days"], 1)
    return o


def preview(repo):
    cmd = ["quai", "--json", "preview", repo.split("#")[0]]
    if repo in patterns:
        cmd += ["--asset-pattern", patterns[repo]]
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    except subprocess.TimeoutExpired:
        return {"ok": False, "error": "timeout"}
    if p.returncode == 0:
        try:
            return {"ok": True, "preview": json.loads(p.stdout)}
        except ValueError:
            pass
    return {"ok": False, "error": (p.stdout + p.stderr).strip()[-600:]}


def one(repo):
    return repo, {"stats": stats(repo), "quai": preview(repo)}


results = {}
with cf.ThreadPoolExecutor(6) as ex:
    for i, (repo, o) in enumerate(ex.map(one, repos)):
        results[repo] = o
        if i % 25 == 0:
            print(i, repo, flush=True)

# dnf test transaction for the most downloaded apps that quai serves.
ranked = sorted((r for r, o in results.items() if o["quai"]["ok"]),
                key=lambda r: -(results[r]["stats"].get("rpm", 0) / results[r]["stats"].get("days", 1)))
for repo in ranked[:TOP]:
    o = results[repo]
    pkgs = [p for p in o["quai"]["preview"]["packages"] if p["arch"] in ("x86_64", "noarch")]
    tests = []
    for p in pkgs:
        url = o["stats"]["urls"].get(p["asset_name"])
        if not url:
            tests.append({"asset": p["asset_name"], "ok": False, "log": "no download URL"}); continue
        path = "/tmp/" + p["asset_name"]
        dl = subprocess.run(["curl", "-sSfL", "--retry", "3", "-o", path, url], capture_output=True, text=True)
        if dl.returncode:
            tests.append({"asset": p["asset_name"], "ok": False, "log": dl.stderr[-400:]}); continue
        t = subprocess.run(["dnf", "-y", "install", "--setopt=tsflags=test", path],
                           capture_output=True, text=True, timeout=900)
        tests.append({"asset": p["asset_name"], "ok": t.returncode == 0,
                      "log": (t.stdout + t.stderr).strip()[-1200:]})
        os.remove(path)
    o["dnf_test"] = tests
    print("dnf", repo, [t["ok"] for t in tests], flush=True)

for o in results.values():
    o["stats"].pop("urls", None)
json.dump({"fedora": open("/etc/os-release").read(), "quai": subprocess.run(
    ["quai", "--version"], capture_output=True, text=True).stdout.strip(), "results": results},
    open(out_path, "w"), indent=1)
print("done", len(results))
