importScripts("/pkg_web_serviceworker/web_worker.js?v=a7e464abf5a0e7c6ac5f1fcae00f8127");
console.log('[DedicatedWorker] IMPORT SCRIPT OK!');

console.log('[DedicatedWorker] wasm_bindgen 1');
const {initDedicatedWorker, computePayloadReply} = wasm_bindgen;
console.log("FUNCTIONS FETCHED FROM wasm_bindgen = ", wasm_bindgen);
console.log("FUNCTION initDedicatedWorker = ", initDedicatedWorker);
console.log("FUNCTION computePayloadReply = ", computePayloadReply);

function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

// If a payload carries a binary `msg_content` (a Uint8Array produced by the wasm
// side), return its backing ArrayBuffer in a transfer list so postMessage moves
// it zero-copy instead of structured-cloning the (300kb-1MB) bytes. Guards against
// error payloads (no msg_content) and detached/empty buffers, where re-transferring
// an already-detached buffer would throw DataCloneError.
function payloadTransferList(payload) {
    const buf = payload && payload.msg_content && payload.msg_content.buffer;
    return buf instanceof ArrayBuffer && buf.byteLength > 0 ? [buf] : [];
}

async function init_wasm_bindgen() {
    console.log('[DedicatedWorker] wasm_bindgen 2');
    await wasm_bindgen("/pkg_web_serviceworker/web_worker_bg.wasm?v=a7e464abf5a0e7c6ac5f1fcae00f8127");
    console.log('[DedicatedWorker] wasm_bindgen done');

      
    let bridgedPort = null;

    console.log('[DedicatedWorker] Dedicated Worker script loaded and initialized.');

    // 1. Direct messages from the tab (e.g. before port is bridged)
    self.onmessage = async (event) => {
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
          await initDedicatedWorker();
          console.info('[DedicatedWorker] initDedicatedWorker() DONE - OK');
          self.postMessage({ type: 'init_result', success: true });
        } catch (err) {
          await sleep(16);
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

        // Set the listener to be async to handle the async computePayloadReply promise
        bridgedPort.addEventListener('message', async (bridgeEvent) => {
          const bridgeData = bridgeEvent.data;
          if (!bridgeData) return;

          // console.log('[DedicatedWorker] Message received from SharedWorker via bridge:', bridgeData);

          if (bridgeData.type === 'execute') {
            const originalPayload = bridgeData.payload;

            try {
              // Perform asynchronous payload modification (prepend "reply: " to all string fields)
              let modifiedPayload = originalPayload;
              if (originalPayload.msg_type) {
                // console.log('[DedicatedWorker] Got Application Message with msg_type = ', originalPayload.msg_type);
                modifiedPayload = await computePayloadReply(originalPayload);
              } else {
                console.error('[DedicatedWorker] Got Payload Without msg_type!', originalPayload);
              }

              // console.log('[DedicatedWorker] Finished processing. Sending reply back:', modifiedPayload);

              // Transfer the reply buffer: computePayloadReply built modifiedPayload
              // freshly from wasm memory and we never touch it again after posting.
              bridgedPort.postMessage({
                type: 'execute_reply',
                seq: bridgeData.seq,
                clientId: bridgeData.clientId,
                is_error: false,
                payload: modifiedPayload
              }, payloadTransferList(modifiedPayload));
            } catch (err) {
              console.error('[DedicatedWorker] Error during computePayloadReply:', err);
              bridgedPort.postMessage({
                type: 'execute_reply',
                seq: bridgeData.seq,
                clientId: bridgeData.clientId,
                is_error: true,
                payload: { is_error: true, error: err.message || 'Processing error' }
              });
            }
          }
        });

        bridgedPort.start();
        console.log('[DedicatedWorker] Bridged port successfully started and listening for messages.');
      }
    };
    console.log('[DedicatedWorker] Worker Loop Initialized.');
    
};

init_wasm_bindgen();

/**
 * dedicated-worker.js
 * 
 * Runs within the leader tab's context. Receives a direct MessagePort from the client,
 * listens to message execution requests routed via the SharedWorker, and returns modified
 * payloads where all string fields are prefixed with "reply: ".
 * 
 * Includes direct initialization checks and direct ping/pong responses.
 */


// /**
//  * Custom initialization function. Throws an error 50% of the time, randomly.
//  */
// function initDedicatedWorker() {
//   const roll = Math.random();
//   console.log(`[DedicatedWorker] initDedicatedWorker roll: ${roll.toFixed(4)}`);
//   if (roll < 0.5) {
//     throw new Error('Random initialization failure (50% chance)');
//   }
//   console.log('[DedicatedWorker] initDedicatedWorker succeeded!');
// }

// /**
//  * Deeply traverses the payload object and prepends "reply: " to every string field.
//  * Handles objects, arrays, and primitive strings.
//  * Now is an asynchronous function returning a Promise.
//  * 
//  * @param {any} payload The original message payload
//  * @returns {Promise<any>} The modified message payload
//  */
// async function computePayloadReply(payload) {
//   // If payload is a direct string, return it modified
//   if (typeof payload === 'string') {
//     return "reply: " + payload;
//   }

//   // If null or not an object, return as-is
//   if (payload === null || typeof payload !== 'object') {
//     return payload;
//   }

//   return computePayloadReply(payload);
// }
