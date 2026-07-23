"""Unit tests for crack_server.rag (claude-context hit normalization)."""

from __future__ import annotations

from crack_server import rag


def test_normalize_hit_maps_claude_context_shape():
    hit = {
        "content": "fn sandbox_overlay() {}",
        "relativePath": "rust_pkg/foo/src/lib.rs",
        "startLine": 12,
        "endLine": 18,
        "language": "rust",
        "score": 0.041,
    }
    out = rag.normalize_hit(hit)
    assert out == {
        "score": 0.041,
        "source": "rust_pkg/foo/src/lib.rs:12-18",
        "snippet": "fn sandbox_overlay() {}",
    }


def test_normalize_hit_falls_back_without_line_range():
    out = rag.normalize_hit({"url": "legacy/path.py", "content": "x", "score": 1})
    assert out["source"] == "legacy/path.py"
    assert out["snippet"] == "x"
    assert out["score"] == 1.0


def test_normalize_hit_missing_score_defaults_zero():
    out = rag.normalize_hit({"relativePath": "a.py", "content": "y"})
    assert out["score"] == 0.0
    assert out["source"] == "a.py"
