#!/usr/bin/env python3
"""A tiny GitHub look-alike for quai's end-to-end tests.

GET /repos/<owner>/<repo>/releases  -> releases from <releases.json>[<owner>/<repo>],
                                       with ETag / If-None-Match (304) support
GET /download/<file>                -> files from --dir, honouring Range requests

releases.json is re-read on every request so tests can "publish" releases.
"""
import argparse
import hashlib
import json
import os
import re
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ARGS = None


def asset_entry(name, base):
    path = os.path.join(ARGS.dir, name)
    data = open(path, "rb").read()
    return {
        "id": int(hashlib.sha256(name.encode()).hexdigest()[:8], 16),
        "name": name,
        "size": len(data),
        "browser_download_url": f"{base}/download/{name}",
        "updated_at": "2026-11-01T10:00:00Z",
        "digest": "sha256:" + hashlib.sha256(data).hexdigest(),
    }


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):  # keep test output readable
        if os.environ.get("MOCK_FORGE_VERBOSE"):
            super().log_message(fmt, *args)

    def send_bytes(self, status, body, ctype, extra=None):
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(body)

    def do_HEAD(self):
        self.do_GET()

    def do_GET(self):
        base = f"http://{self.headers.get('Host')}"
        m = re.fullmatch(r"/repos/([^/]+/[^/]+)/releases(\?.*)?", self.path)
        if m:
            project = m.group(1)
            with open(ARGS.releases) as f:
                table = json.load(f)
            if project not in table:
                return self.send_bytes(404, b'{"message":"Not Found"}', "application/json")
            releases = []
            for r in table[project]:
                rel = dict(r)
                rel["assets"] = [asset_entry(a, base) for a in r.get("assets", [])]
                releases.append(rel)
            body = json.dumps(releases).encode()
            etag = '"' + hashlib.sha256(body).hexdigest()[:16] + '"'
            limits = {"X-RateLimit-Limit": "60", "X-RateLimit-Remaining": "59"}
            if self.headers.get("If-None-Match") == etag:
                self.send_response(304)
                self.send_header("ETag", etag)
                for k, v in limits.items():
                    self.send_header(k, v)
                self.end_headers()
                return
            return self.send_bytes(200, body, "application/json", {"ETag": etag, **limits})

        m = re.fullmatch(r"/download/([A-Za-z0-9._+-]+)", self.path)
        if m:
            path = os.path.join(ARGS.dir, m.group(1))
            if not os.path.isfile(path):
                return self.send_bytes(404, b"not found", "text/plain")
            data = open(path, "rb").read()
            rng = self.headers.get("Range")
            if rng and not ARGS.no_range:
                a, b = rng.split("=", 1)[1].split("-", 1)
                a = int(a)
                b = min(int(b) if b else len(data) - 1, len(data) - 1)
                if a >= len(data):
                    return self.send_bytes(416, b"", "text/plain")
                return self.send_bytes(
                    206, data[a : b + 1], "application/x-rpm",
                    {"Content-Range": f"bytes {a}-{b}/{len(data)}", "Accept-Ranges": "bytes"},
                )
            return self.send_bytes(200, data, "application/x-rpm", {"Accept-Ranges": "bytes"})

        self.send_bytes(404, b"not found", "text/plain")


def main():
    global ARGS
    p = argparse.ArgumentParser()
    p.add_argument("--port", type=int, required=True)
    p.add_argument("--dir", required=True)
    p.add_argument("--releases", required=True)
    p.add_argument("--no-range", action="store_true")
    ARGS = p.parse_args()
    ThreadingHTTPServer(("127.0.0.1", ARGS.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
