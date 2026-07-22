#!/usr/bin/env python3
"""Fake ``pi --mode rpc`` for unit tests.

Env (script-driven, mirrors fake_pi.sh):
  FAKE_PI_DIR     — invocation counter + per-invocation captures
  FAKE_PI_SCRIPT  — one behavior per line per invocation

Argv mode (simple unit tests):
  fake_pi_rpc.py <behavior>   e.g. normal, abort, turns:3
"""
from __future__ import annotations

import json
import os
import sys
import time
from pathlib import Path

_session_messages = 0


def emit(obj: dict) -> None:
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def emit_turn(text: str) -> None:
    emit({"type": "turn_start"})
    emit({
        "type": "message_end",
        "message": {
            "role": "assistant",
            "content": [{"type": "text", "text": text}],
        },
    })
    emit({"type": "turn_end"})


def _record_prompt(text: str) -> int:
    global _session_messages
    _session_messages += 1
    ctrl = os.environ.get("FAKE_PI_DIR")
    if not ctrl:
        return 1
    ctrl_path = Path(ctrl)
    n_file = ctrl_path / "count"
    n = int(n_file.read_text()) if n_file.exists() else 0
    n += 1
    n_file.write_text(str(n))
    (ctrl_path / f"prompt.{n}").write_text(text, encoding="utf-8")
    return n


def _behavior_for_invocation(n: int) -> str:
    script = Path(os.environ["FAKE_PI_SCRIPT"])
    lines = [ln for ln in script.read_text(encoding="utf-8").splitlines() if ln.strip()]
    if not lines:
        return "normal"
    if n - 1 < len(lines):
        return lines[n - 1].strip()
    return lines[-1].strip()


def _parse_behavior(behavior: str) -> tuple[str, str]:
    if ":" in behavior:
        kind, arg = behavior.split(":", 1)
        return kind, arg
    return behavior, ""


def _run_behavior(behavior: str, cmd: dict, inv: int) -> None:
    kind, arg = _parse_behavior(behavior)

    if kind == "promptreject":
        emit({
            "type": "response",
            "command": "prompt",
            "success": False,
            "id": cmd.get("id", "p1"),
            "message": "No project session found",
        })
        emit({"type": "agent_settled"})
        return

    emit({
        "type": "response",
        "command": "prompt",
        "success": True,
        "id": cmd.get("id", "p1"),
    })
    emit({"type": "agent_start"})

    if kind == "normal":
        emit_turn("hello from rpc fake")
        emit({"type": "agent_settled"})
        return

    if kind == "abort":
        emit({"type": "turn_start"})
        time.sleep(0.3)
        while True:
            cmd_line = sys.stdin.readline()
            if not cmd_line:
                break
            incoming = json.loads(cmd_line)
            if incoming.get("type") == "abort":
                break
        emit({"type": "agent_settled"})
        return

    if kind == "turns":
        count = int(arg or "1")
        for i in range(1, count + 1):
            emit_turn(f"turn {i} (invocation {inv})")
        emit({"type": "agent_settled"})
        return

    if kind == "sentinel":
        emit_turn(f"done working\n{arg}")
        emit({"type": "agent_settled"})
        return

    if kind == "inline":
        emit_turn(f"this mentions {arg} mid-line but never alone")
        emit({"type": "agent_settled"})
        return

    if kind == "sleepy":
        emit_turn(f"about to nap (invocation {inv})")
        time.sleep(float(arg or "30"))
        emit({"type": "agent_settled"})
        return

    if kind == "autoretryfail":
        count = int(arg or "1")
        for i in range(1, count + 1):
            emit_turn(f"turn {i} (invocation {inv})")
        emit({
            "type": "auto_retry_end",
            "success": False,
            "finalError": "429 status code (no body)",
        })
        emit({"type": "agent_settled"})
        return

    if kind == "willretry":
        parts = (arg or "2:3").split(":")
        n1, n2 = int(parts[0]), int(parts[1])
        for i in range(1, n1 + 1):
            emit_turn(f"phase1 turn {i} (invocation {inv})")
        emit({"type": "agent_end", "willRetry": True})
        for i in range(1, n2 + 1):
            emit_turn(f"phase2 turn {i} (invocation {inv})")
        emit({"type": "agent_end"})
        emit({"type": "agent_settled"})
        return

    if kind == "write_report":
        import re
        ctrl = os.environ.get("FAKE_PI_DIR", "")
        prompt_text = ""
        if ctrl:
            prompt_text = (Path(ctrl) / f"prompt.{inv}").read_text(encoding="utf-8")
        m = re.search(r"(/[^\s\"']+/report\.md)", prompt_text)
        if m:
            report_path = Path(m.group(1))
            report_path.parent.mkdir(parents=True, exist_ok=True)
            report_path.write_text(
                f"# Report\n\nFake report from invocation {inv}.\n",
                encoding="utf-8",
            )
            emit_turn(f"wrote report to {report_path}")
        else:
            emit_turn(f"no report path found in prompt (invocation {inv})")
        emit({"type": "agent_settled"})
        return

    if kind == "questions":
        emit_turn(
            "Here are clarifying questions:\n\n"
            "```questions\n"
            '[{"id": "q1", "text": "Which approach?", "type": "single", '
            '"options": ["A", "B"]}]\n'
            "```\n"
        )
        emit({"type": "agent_settled"})
        return

    if kind == "concurrent":
        ctrl = Path(os.environ["FAKE_PI_DIR"])
        active_file = ctrl / "active"
        peak_file = ctrl / "peak"
        active = int(active_file.read_text()) if active_file.exists() else 0
        active += 1
        active_file.write_text(str(active))
        peak = int(peak_file.read_text()) if peak_file.exists() else 0
        if active > peak:
            peak_file.write_text(str(active))
        emit_turn(f"concurrent hold (invocation {inv})")
        time.sleep(float(arg or "2"))
        active = max(0, int(active_file.read_text()) - 1)
        active_file.write_text(str(active))
        emit({"type": "agent_settled"})
        return

    if kind == "transient":
        sys.stderr.write("connection reset by peer\n")
        sys.stderr.flush()
        sys.exit(1)

    if kind == "hard":
        sys.stderr.write("boom: unrecoverable parse explosion\n")
        sys.stderr.flush()
        sys.exit(1)

    if kind == "midfail":
        count = int(arg or "1")
        for i in range(1, count + 1):
            emit_turn(f"turn {i} (invocation {inv})")
        sys.stderr.write("connection reset by peer\n")
        sys.stderr.flush()
        sys.exit(1)

    if kind == "midhard":
        count = int(arg or "1")
        for i in range(1, count + 1):
            emit_turn(f"turn {i} (invocation {inv})")
        sys.stderr.write("boom: unrecoverable parse explosion\n")
        sys.stderr.flush()
        sys.exit(1)

    if kind == "crash":
        emit_turn("partial before crash")
        sys.stderr.write("rpc process died\n")
        sys.stderr.flush()
        sys.exit(1)

    if kind == "die":
        count = int(arg or "1")
        for i in range(1, count + 1):
            emit_turn(f"turn {i} (invocation {inv})")
        sys.exit(0)

    if kind == "rpcerror":
        emit({
            "type": "message_update",
            "assistantMessageEvent": {
                "type": "error",
                "error": arg or "529 overloaded_error: Overloaded",
            },
        })
        emit({"type": "agent_settled"})
        return

    print(f"fake_pi_rpc: unknown behavior {behavior!r}", file=sys.stderr)
    sys.exit(2)


def main() -> None:
    argv_behavior = sys.argv[1] if len(sys.argv) > 1 else None

    while True:
        line = sys.stdin.readline()
        if not line:
            return
        cmd = json.loads(line)
        ctype = cmd.get("type")

        if ctype == "set_auto_retry":
            emit({"type": "response", "command": "set_auto_retry", "success": True})
            continue

        if ctype == "get_state":
            emit({
                "type": "response",
                "command": "get_state",
                "success": True,
                "id": cmd.get("id"),
                "data": {"messageCount": _session_messages},
            })
            continue

        if ctype == "prompt":
            inv = _record_prompt(str(cmd.get("message") or ""))
            if os.environ.get("FAKE_PI_SCRIPT"):
                behavior = _behavior_for_invocation(inv)
            else:
                behavior = argv_behavior or "normal"
            _run_behavior(behavior, cmd, inv)
            return

        if ctype == "abort":
            emit({"type": "agent_settled"})
            return


if __name__ == "__main__":
    main()
