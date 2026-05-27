/**
 * shared-worker.js
 * 
 * Tracks active tabs/clients, elects a leader tab randomly to spawn the Dedicated Worker,
 * receives and saves the bridged Dedicated Worker MessagePort, and routes messages
 * between clients and the single Dedicated Worker. Handles failures with exponential backoff.
 * Maintains a message queue to prevent message loss during worker crashes/re-allocations.
 */

let dbWorkerPort = null;
let leaderClientId = null;
const clientPorts = new Map();
let nextClientId = 1;

// Retry configuration for Dedicated Worker initialization
let currentRetryDelay = 60; // Starts at 60ms, doubles on failure
let retryTimeoutId = null;

// Message Queue and active processing state
const messageQueue = [];
let currentProcessingItem = null;
let processingTimeoutId = null;

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
      currentRetryDelay = 60;
      if (retryTimeoutId) {
        clearTimeout(retryTimeoutId);
        retryTimeoutId = null;
      }

      // Handle replies from the Dedicated Worker via the direct port
      dbWorkerPort.addEventListener('message', (dbEvent) => {
        const dbData = dbEvent.data;
        if (!dbData) return;

        console.log('[SharedWorker] Received reply from Dedicated Worker:', dbData);

        if (dbData.type === 'execute_reply') {
          // Clear active processing timeout and complete current item
          if (currentProcessingItem && dbData.clientId === currentProcessingItem.clientId) {
            if (processingTimeoutId) {
              clearTimeout(processingTimeoutId);
              processingTimeoutId = null;
            }
            currentProcessingItem = null;
          }

          const targetPort = clientPorts.get(dbData.clientId);
          if (targetPort) {
            targetPort.postMessage({
              type: 'forwarded_reply',
              payload: dbData.payload
            });
          } else {
            console.warn(`[SharedWorker] Target port for client ${dbData.clientId} no longer exists. Reply dropped.`);
          }

          // Process the next message in the queue
          processQueue();
        }
      });

      dbWorkerPort.start();
      console.log('[SharedWorker] Dedicated Worker Port fully bridged and listening.');
      
      // Trigger queue processing
      processQueue();
      return;
    }

    // 3. Handle Client Message (Queueing)
    if (data.type === 'client_message') {
      console.log(`[SharedWorker] Queueing message from client ${clientId}:`, data.payload);
      messageQueue.push({
        clientId: clientId,
        payload: data.payload
      });
      processQueue();
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
        
        if (processingTimeoutId) {
          clearTimeout(processingTimeoutId);
          processingTimeoutId = null;
        }

        if (currentProcessingItem) {
          console.log('[SharedWorker] Putting active message back to queue due to leader disconnect.');
          messageQueue.unshift(currentProcessingItem);
          currentProcessingItem = null;
        }

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
 * Process the next item in the message queue
 */
function processQueue() {
  if (!dbWorkerPort) {
    console.log('[SharedWorker] Queue processing deferred: Dedicated Worker is not connected.');
    return;
  }

  if (messageQueue.length === 0) {
    return;
  }

  if (currentProcessingItem !== null) {
    console.log('[SharedWorker] Queue is already processing an active message.');
    return;
  }

  // Retrieve next message
  currentProcessingItem = messageQueue.shift();
  const item = currentProcessingItem;
  console.log(`[SharedWorker] Dispatching message for client ${item.clientId} to Dedicated Worker:`, item.payload);

  // Set response timeout (500ms). If no reply comes back, we assume the worker was killed.
  processingTimeoutId = setTimeout(() => {
    console.warn('[SharedWorker] Dedicated Worker message processing timed out! Assuming worker was killed.');
    handleDedicatedWorkerFailure();
  }, 500);

  try {
    dbWorkerPort.postMessage({
      type: 'execute',
      clientId: item.clientId,
      payload: item.payload
    });
  } catch (err) {
    console.error('[SharedWorker] postMessage to Dedicated Worker failed throwing error:', err);
    handleDedicatedWorkerFailure();
  }
}

/**
 * Handles communication failures with the Dedicated Worker.
 * Attempts to instruct the leader tab to terminate its worker.
 * Waits up to 60ms for a SHUTDOWN_OK reply, then elects a new leader randomly.
 */
function handleDedicatedWorkerFailure() {
  if (processingTimeoutId) {
    clearTimeout(processingTimeoutId);
    processingTimeoutId = null;
  }

  const failedItem = currentProcessingItem;
  currentProcessingItem = null;

  // Put the failed item back at the front of the queue
  if (failedItem) {
    console.log('[SharedWorker] Putting failed message back to the front of the queue.');
    messageQueue.unshift(failedItem);
  }

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

    console.log('[SharedWorker] Allocating a new Dedicated Worker on a random tab...');
    electNewLeaderRandomly();
  }

  if (leaderPort) {
    console.log(`[SharedWorker] Sending SHUTDOWN_WORKER request to leader client ${leaderClientId}.`);
    
    // Register 60ms grace period
    shutdownTimeoutId = setTimeout(() => {
      console.warn('[SharedWorker] 60ms grace period expired without SHUTDOWN_OK. Proceeding anyway...');
      proceedToNewAllocation();
    }, 60);

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
