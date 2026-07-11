/**
 * shared-worker.js
 * 
 * Tracks active tabs/clients, elects a leader tab randomly to spawn the Dedicated Worker,
 * receives and saves the bridged Dedicated Worker MessagePort, and routes messages
 * between clients and the single Dedicated Worker. Handles failures with exponential backoff.
 * Maintains a message queue to prevent message loss during worker crashes/re-allocations.
 */

// If a payload carries a binary `msg_content` (a Uint8Array), return its backing
// ArrayBuffer in a transfer list so postMessage moves it zero-copy instead of
// structured-cloning the (300kb-1MB) bytes. Guards against error payloads (no
// msg_content) and detached/empty buffers, where re-transferring an already-
// detached buffer would throw DataCloneError.
function payloadTransferList(payload) {
  const buf = payload && payload.msg_content && payload.msg_content.buffer;
  return buf instanceof ArrayBuffer && buf.byteLength > 0 ? [buf] : [];
}

let dbWorkerPort = null;
let leaderClientId = null;
const clientPorts = new Map();
let nextClientId = 1;

// Retry configuration for Dedicated Worker initialization
let currentRetryDelay = 120; // Starts at 120ms, doubles on failure
let retryTimeoutId = null;

// Message Queue and in-flight processing state.
//
// The Dedicated Worker is fully async: each `execute` message it receives spawns
// an independent task and replies out-of-order as it completes. So we do NOT
// serialize on a single slot — we dispatch every queued message immediately and
// track all outstanding requests concurrently in `inFlight`, keyed by a
// SharedWorker-assigned monotonic `seq`. Each entry owns its own dead-worker
// timeout. Replies are routed back to the originating client by `clientId`; the
// `seq` is only used to find and clear the matching in-flight entry's timer.
//
// `messageQueue` only holds messages that arrived before the Dedicated Worker
// port was bridged (or that got re-queued after a worker failure); once the port
// is live it is drained immediately by `pump()`.
const messageQueue = [];
const inFlight = new Map(); // seq -> { clientId, payload, timeoutId }
let nextSeq = 1;
let reallocating = false; // guards against a failure stampede when many timers fire at once

// How long a single API call may run before we assume the Dedicated Worker is dead.
// This must be generous: real calls fetch + parse remote assets (e.g. the map
// manifest parquet from a remote host), which can take many seconds. A too-small
// value here terminates a perfectly-healthy worker mid-request, and because the
// failed message is re-queued, it loops forever. See handleDedicatedWorkerFailure.
const PROCESSING_TIMEOUT_MS = 120000; // 2 minutes

// Track active shutdown resolver
self.pendingShutdownResolver = null;

console.log('[SharedWorker] Shared Worker script loaded and initialized.');

self.onconnect = (event) => {
  const port = event.ports[0];
  const clientId = nextClientId++;
  clientPorts.set(clientId, port);

  console.log(`[SharedWorker] Client ${clientId} connected. Total clients: ${clientPorts.size}`);

  port.addEventListener('message', (messageEvent) => {
    const data = messageEvent.data;
    if (!data) return;

    // 1. Handle Ping
    if (data.type === 'ping') {
      console.log(`[SharedWorker] Ping received from client ${clientId}. Replying with Pong.`);
      port.postMessage({ type: 'pong', id: data.id });
      return;
    }

    // 2. Register Transferred Dedicated Worker Port
    if (data.type === 'REGISTER_DB_PORT') {
      console.log(`[SharedWorker] Client ${clientId} successfully registered Dedicated Worker Port.`);
      const receivedPort = messageEvent.ports[0];
      
      if (!receivedPort) {
        console.error(`[SharedWorker] Port registration failed: no message port transferred from client ${clientId}.`);
        return;
      }

      dbWorkerPort = receivedPort;
      
      // Reset exponential backoff delay on successful registration
      currentRetryDelay = 120;
      if (retryTimeoutId) {
        clearTimeout(retryTimeoutId);
        retryTimeoutId = null;
      }

      // Handle replies from the Dedicated Worker via the direct port
      dbWorkerPort.addEventListener('message', (dbEvent) => {
        const dbData = dbEvent.data;
        if (!dbData) return;

        // console.log('[SharedWorker] Received reply from Dedicated Worker:', dbData);

        if (dbData.type === 'execute_reply') {
          // Clear the timeout for the matching in-flight request and complete it.
          const entry = inFlight.get(dbData.seq);
          if (entry) {
            clearTimeout(entry.timeoutId);
            inFlight.delete(dbData.seq);
          }

          const targetPort = clientPorts.get(dbData.clientId);
          if (targetPort) {
            // Transfer the reply buffer to the client: this is the terminal hop
            // and the shared worker keeps no reference to the reply payload.
            targetPort.postMessage({
              type: 'forwarded_reply',
              is_error: dbData.is_error,
              payload: dbData.payload
            }, payloadTransferList(dbData.payload));
          } else {
            console.warn(`[SharedWorker] Target port for client ${dbData.clientId} no longer exists. Reply dropped.`);
          }
        }
      });

      dbWorkerPort.start();
      console.log('[SharedWorker] Dedicated Worker Port fully bridged and listening.');

      // Flush any messages that queued up while the worker was unbridged.
      pump();
      return;
    }

    // 3. Handle Client Message (Queueing)
    if (data.type === 'client_message') {
      // console.log(`[SharedWorker] Queueing message from client ${clientId}:`, data.payload);
      messageQueue.push({
        clientId: clientId,
        payload: data.payload
      });
      pump();
      return;
    }

    // 4. Handle Dedicated Worker Initialization Failure
    if (data.type === 'DB_WORKER_INIT_FAILED') {
      console.warn(`[SharedWorker] Client ${clientId} reported Dedicated Worker setup failure (error code: ${data.errorCode}).`);
      
      // Clear out the failed leader reference
      dbWorkerPort = null;
      leaderClientId = null;

      // Exponentially doubling sleep interval
      const sleepDuration = currentRetryDelay;
      console.log(`[SharedWorker] Sleeping for ${sleepDuration}ms before retrying random leader election...`);
      
      currentRetryDelay *= 2; // Double delay for the next failure

      if (retryTimeoutId) clearTimeout(retryTimeoutId);
      retryTimeoutId = setTimeout(() => {
        console.log('[SharedWorker] Retry sleep finished. Selecting a new random leader...');
        electNewLeaderRandomly();
      }, sleepDuration);

      return;
    }

    // 5. Handle SHUTDOWN_OK response from leader tab
    if (data.type === 'SHUTDOWN_OK') {
      console.log(`[SharedWorker] Received SHUTDOWN_OK confirmation from client ${clientId}.`);
      if (typeof self.pendingShutdownResolver === 'function' && clientId === leaderClientId) {
        self.pendingShutdownResolver();
      }
      return;
    }

    // 6. Handle Client Unload / Tab Closing
    if (data.type === 'unload') {
      console.log(`[SharedWorker] Client ${clientId} is unloading.`);
      clientPorts.delete(clientId);

      if (clientId === leaderClientId) {
        console.warn('[SharedWorker] Leader client disconnected. Clearing worker references and electing new leader...');

        // Re-queue every outstanding request so it gets retried on the new worker.
        requeueAllInFlight();

        dbWorkerPort = null;
        leaderClientId = null;
        electNewLeaderRandomly();
      }
      return;
    }
  });

  // Start the port listening
  port.start();

  // If no DB worker is currently active and we are not in a backoff cooldown, elect a leader randomly
  if (!dbWorkerPort && !retryTimeoutId) {
    console.log(`[SharedWorker] No active Dedicated Worker and no pending retry. Electing leader...`);
    electNewLeaderRandomly();
  }
};

/**
 * Drain the message queue, dispatching every pending message to the Dedicated
 * Worker concurrently. Deferred until the worker port is bridged.
 */
function pump() {
  if (!dbWorkerPort) {
    // Not connected yet — messages stay queued and flush from REGISTER_DB_PORT.
    return;
  }

  // Stop if a dispatch failure mid-drain triggered reallocation (dbWorkerPort is
  // torn down / messages get re-queued for the next worker).
  while (dbWorkerPort && !reallocating && messageQueue.length > 0) {
    dispatch(messageQueue.shift());
  }
}

/**
 * Send a single message to the Dedicated Worker, tracking it as in-flight with
 * its own dead-worker timeout. Does not block other messages.
 */
function dispatch(item) {
  const seq = nextSeq++;
  // console.log(`[SharedWorker] Dispatching message seq=${seq} for client ${item.clientId}:`, item.payload);

  // Per-message response timeout. If no reply comes back, we assume the worker was killed.
  const timeoutId = setTimeout(() => {
    console.warn(`[SharedWorker] Dedicated Worker message processing timed out after ${PROCESSING_TIMEOUT_MS}ms (seq=${seq}, msg_type=${item.payload && item.payload.msg_type})! Assuming worker was killed.`);
    handleDedicatedWorkerFailure();
  }, PROCESSING_TIMEOUT_MS);

  inFlight.set(seq, {
    clientId: item.clientId,
    payload: item.payload,
    timeoutId: timeoutId
  });

  try {
    // NOTE: deliberately NOT transferring item.payload.msg_content here. This
    // payload is retained in `inFlight` and may be re-dispatched to a fresh worker
    // if the current one dies (see requeueAllInFlight). Transferring would detach
    // the buffer and silently re-send empty content on retry, so the request leg
    // pays one structured clone in exchange for crash-retry correctness.
    dbWorkerPort.postMessage({
      type: 'execute',
      seq: seq,
      clientId: item.clientId,
      payload: item.payload
    });
  } catch (err) {
    console.error('[SharedWorker] postMessage to Dedicated Worker failed throwing error:', err);
    handleDedicatedWorkerFailure();
  }
}

/**
 * Clear all in-flight timers and move every outstanding request back to the
 * front of the queue so it is retried on the next Dedicated Worker.
 */
function requeueAllInFlight() {
  if (inFlight.size === 0) return;
  console.log(`[SharedWorker] Re-queueing ${inFlight.size} in-flight message(s) for retry.`);
  for (const entry of inFlight.values()) {
    clearTimeout(entry.timeoutId);
    messageQueue.unshift({ clientId: entry.clientId, payload: entry.payload });
  }
  inFlight.clear();
}

/**
 * Handles communication failures with the Dedicated Worker.
 * Attempts to instruct the leader tab to terminate its worker.
 * Waits up to 120ms for a SHUTDOWN_OK reply, then elects a new leader randomly.
 */
function handleDedicatedWorkerFailure() {
  // Many in-flight messages can time out (or fail to post) at once when the
  // worker actually dies. Only run the reallocation once until it completes.
  if (reallocating) return;
  reallocating = true;

  // Re-queue every outstanding request so it gets retried on the new worker.
  requeueAllInFlight();

  const leaderPort = clientPorts.get(leaderClientId);
  let resolved = false;
  let shutdownTimeoutId = null;

  function proceedToNewAllocation() {
    if (resolved) return;
    resolved = true;

    if (shutdownTimeoutId) {
      clearTimeout(shutdownTimeoutId);
      shutdownTimeoutId = null;
    }

    self.pendingShutdownResolver = null;
    dbWorkerPort = null;
    leaderClientId = null;

    // Allow the next (new) worker to fail independently.
    reallocating = false;

    console.log('[SharedWorker] Allocating a new Dedicated Worker on a random tab...');
    electNewLeaderRandomly();
  }

  if (leaderPort) {
    console.log(`[SharedWorker] Sending SHUTDOWN_WORKER request to leader client ${leaderClientId}.`);
    
    // Register 120ms grace period
    shutdownTimeoutId = setTimeout(() => {
      console.warn('[SharedWorker] 120ms grace period expired without SHUTDOWN_OK. Proceeding anyway...');
      proceedToNewAllocation();
    }, 120);

    self.pendingShutdownResolver = () => {
      console.log('[SharedWorker] SHUTDOWN_OK received within grace period.');
      proceedToNewAllocation();
    };

    try {
      leaderPort.postMessage({ type: 'SHUTDOWN_WORKER' });
    } catch (err) {
      console.warn('[SharedWorker] Failed to post SHUTDOWN_WORKER message to leader port:', err);
      proceedToNewAllocation();
    }
  } else {
    console.log('[SharedWorker] Leader port is not available. Proceeding directly to reallocation.');
    proceedToNewAllocation();
  }
}

/**
 * Elects a random leader tab from all currently connected client ports
 * to attempt spawning and configuring a Dedicated Worker.
 */
function electNewLeaderRandomly() {
  if (clientPorts.size === 0) {
    console.log('[SharedWorker] No clients remaining. Leader election aborted.');
    return;
  }

  // Pick a random client ID
  const activeIds = Array.from(clientPorts.keys());
  const randomIndex = Math.floor(Math.random() * activeIds.length);
  const randomLeaderId = activeIds[randomIndex];
  const randomLeaderPort = clientPorts.get(randomLeaderId);

  leaderClientId = randomLeaderId;
  console.log(`[SharedWorker] Randomly elected client ${randomLeaderId} as leader to spawn Dedicated Worker.`);

  // Prompt the randomly elected leader client to spawn the dedicated worker
  randomLeaderPort.postMessage({ type: 'NEED_DB_WORKER' });
}
