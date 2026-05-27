/**
 * dedicated-worker.js
 * 
 * Runs within the leader tab's context. Receives a direct MessagePort from the client,
 * listens to message execution requests routed via the SharedWorker, and returns modified
 * payloads where all string fields are prefixed with "reply: ".
 * 
 * Includes direct initialization checks and direct ping/pong responses.
 */

let bridgedPort = null;

console.log('[DedicatedWorker] Dedicated Worker script loaded and initialized.');

// 1. Direct messages from the tab (e.g. before port is bridged)
self.onmessage = (event) => {
  const data = event.data;
  if (!data) return;

  // Direct Ping Handling
  if (data.type === 'ping') {
    console.log(`[DedicatedWorker] Direct ping received with ID: ${data.id}. Replying with Pong.`);
    self.postMessage({ type: 'pong', id: data.id });
    return;
  }

  // Direct Custom Initialization Handling
  if (data.type === 'init_dedicated_worker') {
    console.log('[DedicatedWorker] Custom initialization requested. Running initDedicatedWorker()...');
    try {
      initDedicatedWorker();
      self.postMessage({ type: 'init_result', success: true });
    } catch (err) {
      console.error('[DedicatedWorker] Custom initialization error caught:', err.message);
      self.postMessage({ type: 'init_result', success: false, error: err.message });
    }
    return;
  }

  // Port Bridging Command
  if (data.type === 'INIT_PORT') {
    console.log('[DedicatedWorker] INIT_PORT received. Initializing bridged port.');
    bridgedPort = event.ports[0];

    if (!bridgedPort) {
      console.error('[DedicatedWorker] Failed to initialize bridged port: no port transferred.');
      return;
    }

    bridgedPort.addEventListener('message', (bridgeEvent) => {
      const bridgeData = bridgeEvent.data;
      if (!bridgeData) return;

      console.log('[DedicatedWorker] Message received from SharedWorker via bridge:', bridgeData);

      if (bridgeData.type === 'execute') {
        const originalPayload = bridgeData.payload;

        // Perform payload modification (prepend "reply: " to all string fields)
        const modifiedPayload = computePayloadReply(originalPayload);

        console.log('[DedicatedWorker] Finished processing. Sending reply back:', modifiedPayload);

        bridgedPort.postMessage({
          type: 'execute_reply',
          clientId: bridgeData.clientId,
          payload: modifiedPayload
        });
      }
    });

    bridgedPort.start();
    console.log('[DedicatedWorker] Bridged port successfully started and listening for messages.');
  }
};

/**
 * Custom initialization function. Throws an error 50% of the time, randomly.
 */
function initDedicatedWorker() {
  const roll = Math.random();
  console.log(`[DedicatedWorker] initDedicatedWorker roll: ${roll.toFixed(4)}`);
  if (roll < 0.5) {
    throw new Error('Random initialization failure (50% chance)');
  }
  console.log('[DedicatedWorker] initDedicatedWorker succeeded!');
}

/**
 * Deeply traverses the payload object and prepends "reply: " to every string field.
 * Handles objects, arrays, and primitive strings.
 * 
 * @param {any} payload The original message payload
 * @returns {any} The modified message payload
 */
function computePayloadReply(payload) {
  // If payload is a direct string, return it modified
  if (typeof payload === 'string') {
    return "reply: " + payload;
  }

  // If null or not an object, return as-is
  if (payload === null || typeof payload !== 'object') {
    return payload;
  }

  // Deep clone to prevent mutating original objects
  let cloned;
  try {
    cloned = JSON.parse(JSON.stringify(payload));
  } catch (e) {
    // Fallback simple clone if circular or non-serializable
    cloned = Array.isArray(payload) ? [...payload] : { ...payload };
  }

  // Recursive traversal to modify all string values
  function traverse(obj) {
    for (const key in obj) {
      if (Object.prototype.hasOwnProperty.call(obj, key)) {
        if (typeof obj[key] === 'string') {
          obj[key] = "reply: " + obj[key];
        } else if (obj[key] !== null && typeof obj[key] === 'object') {
          traverse(obj[key]);
        }
      }
    }
  }

  traverse(cloned);
  return cloned;
}
