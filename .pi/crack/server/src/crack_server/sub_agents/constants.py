"""Sub-agent package constants (leaf module — no internal imports)."""

SUBAGENT_JOB_SLUG = "__subagent__"
MAX_DEPTH = 2
SUBAGENT_TIMEOUT_SECONDS = 3600
# Grace before a running phase with no queued job is flagged orphaned.
ORPHAN_PHASE_GRACE_SECONDS = 10.0
