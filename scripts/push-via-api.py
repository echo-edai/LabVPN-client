#!/usr/bin/env python3
"""GitHub HTTPS 被网络阻断时，用 GitHub API 推送当前本地提交（含可选 tag）。

原理：用本地 git 计算相对「上次已推送提交」的差异，通过 git data API 创建 blob/tree/commit，
再更新远端分支与 tag。仅用于本仓库维护，不包含任何凭据。

用法：
  OLD_REMOTE=<远端分支当前 sha> BASE_LOCAL=<与远端内容一致的本地提交> \
  NEW_LOCAL=$(git rev-parse HEAD) TAG=v0.1.8 TAG_MSG="..." \
  SSL_CERT_FILE=/etc/ssl/cert.pem python3 scripts/push-via-api.py

依赖：git、gh（提供 token）、Python 3。
"""
import base64
import json
import os
import subprocess
import sys
import urllib.error
import urllib.request

REPO = os.environ.get("REPO", "echo-edai/LabVPN-client")
OLD_REMOTE = os.environ["OLD_REMOTE"]
BASE_LOCAL = os.environ["BASE_LOCAL"]
NEW_LOCAL = os.environ.get("NEW_LOCAL") or subprocess.check_output(
    ["git", "rev-parse", "HEAD"], text=True
).strip()
TAG = os.environ.get("TAG", "")
TAG_MSG = os.environ.get("TAG_MSG", TAG)
TOKEN = os.environ.get("GITHUB_TOKEN") or subprocess.check_output(
    ["gh", "auth", "token"], text=True
).strip()


def api(method, path, payload=None):
    url = f"https://api.github.com{path}"
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {TOKEN}")
    req.add_header("Accept", "application/vnd.github+json")
    req.add_header("User-Agent", "labvpn-push")
    if data:
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as exc:
        raise RuntimeError(f"HTTP {exc.code} {method} {path}: {exc.read().decode()[:600]}")


def git(*args):
    return subprocess.check_output(["git", *args], text=True)


def main():
    diff = git("diff", "--name-status", f"{BASE_LOCAL}..{NEW_LOCAL}").strip().splitlines()
    changes = []
    for line in diff:
        parts = line.split("\t")
        if len(parts) >= 2:
            changes.append((parts[0], parts[-1]))
    print(f"变更文件：{len(changes)}")

    base_tree = api("GET", f"/repos/{REPO}/git/commits/{OLD_REMOTE}")["tree"]["sha"]
    entries = []
    for status, path in changes:
        if status == "D":
            entries.append({"path": path, "mode": "100644", "type": "blob", "sha": None})
            continue
        content = subprocess.check_output(["git", "show", f"{NEW_LOCAL}:{path}"])
        mode = (git("ls-tree", NEW_LOCAL, path).split() or ["100644"])[0]
        blob = api(
            "POST",
            f"/repos/{REPO}/git/blobs",
            {"content": base64.b64encode(content).decode(), "encoding": "base64"},
        )
        entries.append({"path": path, "mode": mode, "type": "blob", "sha": blob["sha"]})

    tree = api("POST", f"/repos/{REPO}/git/trees", {"base_tree": base_tree, "tree": entries})
    message = git("log", "-1", "--format=%B", NEW_LOCAL).strip()
    name = git("config", "user.name").strip() or "LabVPN"
    email = git("config", "user.email").strip() or "noreply@users.noreply.github.com"
    commit = api(
        "POST",
        f"/repos/{REPO}/git/commits",
        {
            "message": message,
            "tree": tree["sha"],
            "parents": [OLD_REMOTE],
            "author": {"name": name, "email": email},
            "committer": {"name": name, "email": email},
        },
    )
    new_sha = commit["sha"]
    api("PATCH", f"/repos/{REPO}/git/refs/heads/main", {"sha": new_sha, "force": False})
    print("main ->", new_sha)
    if TAG:
        tag_obj = api(
            "POST",
            f"/repos/{REPO}/git/tags",
            {"tag": TAG, "message": TAG_MSG, "object": new_sha, "type": "commit"},
        )
        try:
            api("PATCH", f"/repos/{REPO}/git/refs/tags/{TAG}", {"sha": tag_obj["sha"], "force": True})
        except RuntimeError:
            api("POST", f"/repos/{REPO}/git/refs", {"ref": f"refs/tags/{TAG}", "sha": tag_obj["sha"]})
        print("tag ->", tag_obj["sha"])
    print("REMOTE_SHA=" + new_sha)


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # noqa: BLE001
        print(f"推送失败：{exc}", file=sys.stderr)
        sys.exit(1)
