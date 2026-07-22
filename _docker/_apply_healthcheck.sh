#!/bin/bash
# Plan 7 Part B (steps 3-4): after a self-modifying patch is applied to the live
# crack-dev /workspace, uvicorn reloads the crack-server package. If the reloaded
# server never comes healthy, reverse-apply the patch so the reloader recovers to
# the previous good tree.
#
# Launched detached (start_new_session) by patch.launch_health_watcher, so it
# survives the very reload it is watching. Never touches the tree on success.
#
# args: <patch_path> <chat_id> [deadline_seconds]
set -u

PATCH="${1:?patch path required}"
CHAT="${2:?chat id required}"
DEADLINE="${3:-60}"

WORKSPACE="${CRACK_HOST_REPO_ROOT:-/workspace}"
# Inside crack-dev the repo is /workspace; the health endpoint is local.
[ -d /workspace/.git ] && WORKSPACE=/workspace
HEALTH_URL="http://127.0.0.1:${CRACK_PI_PORT:-9847}/"
LOGDIR="${CRACK_HARNESS_DATA_DIR:-/crack-harness-data}/harness"
mkdir -p "$LOGDIR" 2>/dev/null || true
LOG="$LOGDIR/apply_rollback.log"

log() { echo "$(date -Is) [healthcheck chat=$CHAT] $*" >>"$LOG" 2>/dev/null || true; }

# Give the file-watch reload a moment to notice the applied change.
sleep 5

deadline=$((SECONDS + DEADLINE))
while [ "$SECONDS" -lt "$deadline" ]; do
    code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "$HEALTH_URL" 2>/dev/null || echo 000)
    if [ "$code" = "200" ]; then
        log "healthy after self-mod apply of $PATCH — keeping change"
        exit 0
    fi
    sleep 3
done

# Unhealthy past the deadline: revert so the reloader restores a working server.
log "UNHEALTHY ${DEADLINE}s after applying $PATCH — reverse-applying to roll back"
if git -C "$WORKSPACE" apply -R "$PATCH" >>"$LOG" 2>&1; then
    log "reverse-apply OK; server should reload back to a good tree"
    # Leave a breadcrumb in the chat dir for the post-mortem.
    CHATDIR="${CRACK_HARNESS_DATA_DIR:-/crack-harness-data}/unscripted_chats/$CHAT"
    [ -d "$CHATDIR" ] && echo "self-mod patch $PATCH rolled back: server did not come healthy" \
        >"$CHATDIR/APPLY_ROLLBACK.txt" 2>/dev/null || true
else
    log "reverse-apply FAILED — manual intervention needed for $PATCH"
fi
exit 1
