#!/usr/bin/env bash
# Fake `pi` for tests — copied onto PATH as `pi`, ahead of the real binary.
#
# Env:
#   FAKE_PI_DIR    — dir for the invocation counter and per-invocation captures
#   FAKE_PI_SCRIPT — file with one behavior per line, one line per invocation;
#                    the last line repeats for extra invocations.
#
# Every invocation appends its argv (one arg per line) to $FAKE_PI_DIR/argv.<n>
# and its prompt (the last argument) to $FAKE_PI_DIR/prompt.<n>.
#
# Behaviors:
#   turns:N        (json mode) emit N text turns then agent_end
#   sentinel:STR   emit one turn ending with STR on its own line, then agent_end
#   inline:STR     emit one turn with STR embedded mid-line, then agent_end
#   sleepy:N       emit one turn, sleep N seconds, then agent_end
#   turnsgap:N:M   emit N turns with M seconds between each, then agent_end
#   transient      print a transient-looking error to stderr and exit 1
#   midfail:N      emit N turns, then print "connection reset" and exit 1
#   hard           print a non-transient error to stderr and exit 1
#   ok             (print mode) echo "text-response" and exit 0
#   copy:SRC>DST   copy file SRC to DST (an "agent wrote the artifact" stand-in),
#                  emit one turn, then agent_end
set -u

n_file="$FAKE_PI_DIR/count"
n=$(cat "$n_file" 2>/dev/null || echo 0)
n=$((n + 1))
echo "$n" > "$n_file"

printf '%s\n' "$@" > "$FAKE_PI_DIR/argv.$n"
last=""
for a in "$@"; do last="$a"; done
printf '%s' "$last" > "$FAKE_PI_DIR/prompt.$n"

line=$(sed -n "${n}p" "$FAKE_PI_SCRIPT")
if [ -z "$line" ]; then
  line=$(tail -n 1 "$FAKE_PI_SCRIPT")
fi
behavior="${line%%:*}"
arg="${line#*:}"

emit_turn() { # $1 = text (JSON-escaped via python for safety)
  json_text=$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$1")
  printf '{"type":"turn_start"}\n'
  printf '{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":%s}]}}\n' "$json_text"
  printf '{"type":"turn_end"}\n'
}

case "$behavior" in
  turns)
    for i in $(seq 1 "$arg"); do
      emit_turn "turn $i (invocation $n)"
    done
    printf '{"type":"agent_end"}\n'
    ;;
  sentinel)
    emit_turn "done working
$arg"
    printf '{"type":"agent_end"}\n'
    ;;
  inline)
    emit_turn "this mentions $arg mid-line but never alone"
    printf '{"type":"agent_end"}\n'
    ;;
  sleepy)
    emit_turn "about to nap (invocation $n)"
    sleep "$arg"
    printf '{"type":"agent_end"}\n'
    ;;
  turnsgap)
    count="${arg%%:*}"
    gap="${arg#*:}"
    for i in $(seq 1 "$count"); do
      emit_turn "turn $i (invocation $n)"
      [ "$i" -lt "$count" ] && sleep "$gap"
    done
    printf '{"type":"agent_end"}\n'
    ;;
  transient)
    echo "ResourceExhausted: quota exceeded, retry later" >&2
    exit 1
    ;;
  midfail)
    for i in $(seq 1 "$arg"); do
      emit_turn "turn $i (invocation $n)"
    done
    echo "connection reset by peer" >&2
    exit 1
    ;;
  midhard)
    for i in $(seq 1 "$arg"); do
      emit_turn "turn $i (invocation $n)"
    done
    echo "boom: unrecoverable parse explosion" >&2
    exit 1
    ;;
  hard)
    echo "boom: unrecoverable parse explosion" >&2
    exit 1
    ;;
  ok)
    echo "text-response"
    ;;
  copy)
    src="${arg%%>*}"
    dst="${arg#*>}"
    mkdir -p "$(dirname "$dst")"
    cp "$src" "$dst"
    emit_turn "wrote the artifact (invocation $n)"
    printf '{"type":"agent_end"}\n'
    ;;
  write_report)
    # Extract an absolute …/report.md path from the prompt and write a stub report.
    report_path=$(python3 -c '
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r"(/[^\s\"\047]+/report\.md)", text)
if not m:
    m = re.search(r"([A-Za-z]:\\\\[^\s\"\047]+\\\\report\.md)", text)
print(m.group(1) if m else "")
' "$FAKE_PI_DIR/prompt.$n")
    if [ -n "$report_path" ]; then
      mkdir -p "$(dirname "$report_path")"
      printf '# Report\n\nFake report from invocation %s.\n' "$n" > "$report_path"
      emit_turn "wrote report to $report_path"
    else
      emit_turn "no report path found in prompt (invocation $n)"
    fi
    printf '{"type":"agent_end"}\n'
    ;;
  questions)
    # Emit a valid ```questions block for planner grill tests.
    emit_turn "Here are clarifying questions:

\`\`\`questions
[
  {\"id\": \"q1\", \"text\": \"Which approach?\", \"type\": \"single\", \"options\": [\"A\", \"B\"]}
]
\`\`\`
"
    printf '{"type":"agent_end"}\n'
    ;;
  *)
    echo "fake_pi: unknown behavior '$line'" >&2
    exit 2
    ;;
esac
