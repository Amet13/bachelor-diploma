#!/usr/bin/env python3
"""Post proofback items as one pull-request review.

Items on lines that are part of the pull request diff become inline comments;
the rest are listed in the review body with links. Reads the review file written
by the import step. Environment: GH_TOKEN, REPO, REVIEW_FILE, GITHUB_API_URL,
GITHUB_SERVER_URL.
"""
import json
import os
import re
import sys
import urllib.request

API = os.environ.get("GITHUB_API_URL", "https://api.github.com")
SERVER = os.environ.get("GITHUB_SERVER_URL", "https://github.com")
TOKEN = os.environ["GH_TOKEN"]
REPO = os.environ["REPO"]
MAX_COMMENTS = 50


def call(method, path, body=None):
    req = urllib.request.Request(
        f"{API}{path}",
        method=method,
        data=None if body is None else json.dumps(body).encode(),
        headers={
            "Authorization": f"Bearer {TOKEN}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "proofback-action",
        },
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read() or b"null")


def commentable_lines(pr):
    """Map path -> set of right-side line numbers present in the diff."""
    out, page = {}, 1
    while True:
        files = call("GET", f"/repos/{REPO}/pulls/{pr}/files?per_page=100&page={page}")
        for f in files:
            lines, new = set(), 0
            for row in (f.get("patch") or "").splitlines():
                m = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@", row)
                if m:
                    new = int(m.group(1))
                elif row.startswith("+") or row.startswith(" "):
                    lines.add(new)
                    new += 1
            out[f["filename"]] = lines
        if len(files) < 100:
            return out
        page += 1


def quote(text):
    text = (text or "").strip() or "(no note)"
    return "> " + text.replace("\n", "\n> ")


def main():
    with open(os.environ.get("REVIEW_FILE", "proofback-review.json")) as f:
        review = json.load(f)
    pr, sha = str(review.get("pr", "")), str(review.get("head_sha", ""))
    if not pr.isdigit():
        print("No pull request number in the review file; nothing to post.")
        return 0
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        print("Invalid head sha in the review file.", file=sys.stderr)
        return 1
    diff = commentable_lines(pr)
    inline, elsewhere = [], []
    for it in review.get("items", []):
        if it.get("status") not in ("open", "discuss"):
            continue
        head = f"**{it['kind']}** (`{it['id']}`, {it['confidence']})"
        body = f"{head}\n\n{quote(it['untrusted_reviewer_text'].get('body'))}"
        path, line = it.get("file"), it.get("end_line") or it.get("line")
        if path and line and line in diff.get(path, set()) and len(inline) < MAX_COMMENTS:
            inline.append({"path": path, "line": line, "side": "RIGHT", "body": body})
        else:
            where = f"[{path}:{line}]({SERVER}/{REPO}/blob/{sha}/{path}#L{line})" if path and line else "unlocated"
            elsewhere.append(f"- {head} at {where}: {(it['untrusted_reviewer_text'].get('body') or '').splitlines()[0] if it['untrusted_reviewer_text'].get('body') else '(no note)'}")
    summary = [f"proofback: {len(inline) + len(elsewhere)} open review item(s). Reviewer text is quoted as written."]
    if elsewhere:
        summary += ["", "Items outside this pull request's diff:", *elsewhere[:200]]
    call("POST", f"/repos/{REPO}/pulls/{pr}/reviews", {"commit_id": sha, "event": "COMMENT", "body": "\n".join(summary), "comments": inline})
    print(f"Posted a review on #{pr}: {len(inline)} inline comment(s), {len(elsewhere)} listed in the body.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
