"""Tests for post_review.py with the GitHub API mocked out."""
import json
import os
import tempfile
import unittest
from unittest import mock

os.environ.setdefault("GH_TOKEN", "x")
os.environ.setdefault("REPO", "o/r")
import post_review  # noqa: E402

PATCH = "@@ -10,3 +10,4 @@\n line ten\n+added eleven\n line twelve\n line thirteen"


class PostReviewTest(unittest.TestCase):
    def run_with(self, items):
        posted = {}

        def fake(method, path, body=None):
            if method == "GET":
                return [{"filename": "a.tex", "patch": PATCH}]
            posted["body"] = body
            return {}

        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump({"pr": "7", "head_sha": "a" * 40, "items": items}, f)
        os.environ["REVIEW_FILE"] = f.name
        with mock.patch.object(post_review, "call", fake):
            self.assertEqual(post_review.main(), 0)
        os.unlink(f.name)
        return posted.get("body")

    def item(self, **kw):
        base = {"id": "pb_1", "kind": "comment", "status": "open", "confidence": "exact",
                "file": "a.tex", "line": 11, "end_line": 11,
                "untrusted_reviewer_text": {"body": "Line one\nline two"}}
        base.update(kw)
        return base

    def test_inline_and_elsewhere(self):
        body = self.run_with([self.item(), self.item(id="pb_2", line=40, end_line=40), self.item(id="pb_3", status="done")])
        self.assertEqual(len(body["comments"]), 1)
        self.assertEqual(body["comments"][0]["line"], 11)
        self.assertIn("> Line one\n> line two", body["comments"][0]["body"])
        self.assertIn("pb_2", body["body"])
        self.assertNotIn("pb_3", body["body"])

    def test_no_pr_posts_nothing(self):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump({"pr": "", "head_sha": "a" * 40, "items": []}, f)
        os.environ["REVIEW_FILE"] = f.name
        with mock.patch.object(post_review, "call", side_effect=AssertionError("no API calls")):
            self.assertEqual(post_review.main(), 0)
        os.unlink(f.name)


if __name__ == "__main__":
    unittest.main()
