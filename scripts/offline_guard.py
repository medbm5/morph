#!/usr/bin/env python3
"""Offline guard: fail if Metamorphosis code can reach the network outside the updater.

Two checks:
1. Dependency graph — no workspace crate may depend (directly or transitively) on an
   HTTP/socket crate, except through `morph-updater` (app updates and optional component
   downloads) or through the vetted Tauri framework, whose bundled HTTP client our code
   never calls.
2. Source scan — no Rust or UI source outside `crates/morph-updater` may use networking
   APIs (std::net, reqwest, fetch, XMLHttpRequest, WebSocket, …).
"""
import json
import pathlib
import re
import subprocess
import sys

NETWORK_CRATES = {
    "reqwest", "hyper", "ureq", "curl", "isahc", "surf", "attohttpc", "minreq",
    "tokio-tungstenite", "tungstenite", "native-tls", "rustls", "h2", "quinn",
}
ALLOWED_ROOTS = {"morph-updater"}
VETTED_FRAMEWORKS = {"tauri"}

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE_PATTERNS = {
    ".rs": re.compile(r"\b(std::net|TcpStream|TcpListener|UdpSocket|reqwest::|hyper::|ureq::)"),
    ".js": re.compile(r"\b(fetch\s*\(|XMLHttpRequest|WebSocket\s*\(|navigator\.sendBeacon|EventSource\s*\()"),
    ".html": re.compile(r"(https?://(?!www\.w3\.org)[^\s\"'<>]+\.(?:js|css|woff2?)|fonts\.googleapis)"),
}
SCANNED_DIRS = ["crates", "app/src-tauri/src", "app/ui"]
SKIPPED_DIRS = {"morph-updater", "target", "node_modules"}


def dependency_failures() -> list[str]:
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1"], cwd=ROOT))
    names = {p["id"]: p["name"] for p in meta["packages"]}
    graph = {n["id"]: [d["pkg"] for d in n["deps"]] for n in meta["resolve"]["nodes"]}
    stop = ALLOWED_ROOTS | VETTED_FRAMEWORKS

    failures = []
    for root in sorted(meta["workspace_members"], key=lambda i: names[i]):
        if names[root] in ALLOWED_ROOTS:
            continue
        stack, parent = [root], {root: None}
        while stack:
            node = stack.pop()
            for dep in graph.get(node, []):
                if dep in parent or names[dep] in stop:
                    continue
                parent[dep] = node
                if names[dep] in NETWORK_CRATES:
                    path, cur = [], dep
                    while cur is not None:
                        path.append(names[cur])
                        cur = parent[cur]
                    failures.append(" -> ".join(reversed(path)))
                stack.append(dep)
    return failures


def source_failures() -> list[str]:
    failures = []
    for d in SCANNED_DIRS:
        base = ROOT / d
        if not base.exists():
            continue
        for path in base.rglob("*"):
            if not path.is_file() or path.suffix not in SOURCE_PATTERNS:
                continue
            if SKIPPED_DIRS & set(path.relative_to(ROOT).parts):
                continue
            for n, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
                if "offline-guard: allow" in line:
                    continue
                if SOURCE_PATTERNS[path.suffix].search(line):
                    failures.append(f"{path.relative_to(ROOT)}:{n}: {line.strip()[:120]}")
    return failures


def main() -> int:
    deps, src = dependency_failures(), source_failures()
    for title, items in (("Network crates reachable outside morph-updater:", deps),
                         ("Network APIs used outside morph-updater:", src)):
        if items:
            print(title)
            for f in items:
                print("  " + f)
    if deps or src:
        return 1
    print("offline guard: no network access outside morph-updater")
    return 0


if __name__ == "__main__":
    sys.exit(main())
