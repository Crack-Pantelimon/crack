/**
 * client.js
 * 
 * ============================================================================
 * MESSAGE SCHEMES AND COMMUNICATION PLAN
 * ============================================================================
 * 
 * 1. Ping / Pong (Connection Verification - Shared & Dedicated Workers)
 *    - Sender -> Receiver: { type: 'ping', id: <unique_id> }
 *    - Receiver -> Sender: { type: 'pong', id: <unique_id> }
 *    - Purpose: Verify if the Shared Worker or Dedicated Worker is alive. Retried up to 10 times.
 * 
 * 2. Dedicated Worker Allocation
 *    - Shared Worker -> Client (Leader): { type: 'NEED_DB_WORKER' }
 *    - Purpose: Ask the elected leader client tab to spawn the Dedicated Worker.
 * 
 * 3. Dedicated Worker Custom Initialization
 *    - Client (Leader) -> Dedicated Worker: { type: 'init_dedicated_worker' }
 *    - Dedicated Worker -> Client (Leader): { type: 'init_result', success: <boolean>, error: <string> }
 *    - Purpose: Run internal dedicated worker initialization (initDedicatedWorker).
 * 
 * 4. Dedicated Worker Port Switching / Registration
 *    - Client (Leader) -> Dedicated Worker: { type: 'INIT_PORT' } with message port [port1]
 *    - Client (Leader) -> Shared Worker: { type: 'REGISTER_DB_PORT' } with message port [port2]
 *    - Purpose: Bridge the Dedicated Worker and the Shared Worker directly via MessageChannel.
 * 
 * 5. Initialization Error Notification
 *    - Client (Leader) -> Shared Worker: { type: 'DB_WORKER_INIT_FAILED', errorCode: <string> }
 *    - Purpose: Inform Shared Worker of a setup/init failure, triggering backoff and retry.
 * 
 * 6. Shut Down Request
 *    - Shared Worker -> Client (Leader): { type: 'SHUTDOWN_WORKER' }
 *    - Client (Leader) -> Shared Worker: { type: 'SHUTDOWN_OK' }
 *    - Purpose: Gracefully terminate the Dedicated Worker on demand if the connection hangs/crashes.
 * 
 * 7. Client Sending Messages
 *    - Client -> Shared Worker: { type: 'client_message', payload: <message_obj> }
 *    - Purpose: Client submits a query/message to the system.
 * 
 * 8. Routing to Dedicated Worker
 *    - Shared Worker -> Dedicated Worker (via Port 2): { type: 'execute', clientId: <clientId>, payload: <message_obj> }
 *    - Purpose: Forward the client's message to the Dedicated Worker, tagging it with the sender's clientId.
 * 
 * 9. Replying from Dedicated Worker
 *    - Dedicated Worker -> Shared Worker (via Port 1): { type: 'execute_reply', clientId: <clientId>, payload: <modified_message_obj> }
 *    - Purpose: Dedicated Worker responds with the modified payload (string fields prefixed with "reply: ").
 * 
 * 10. Routing Back to Client
 *    - Shared Worker -> Client: { type: 'forwarded_reply', payload: <modified_message_obj> }
 *    - Purpose: Deliver the final response to the originating client.
 * 
 * 11. Cleanup on Tab Unload
 *    - Client -> Shared Worker: { type: 'unload' }
 *    - Purpose: Notify the Shared Worker that the tab is closing. If this was the leader, a new leader is elected.
 * ============================================================================
 */

export function init_workers2() {
    console.log('[Client] init_workers2() invoked.');

    // Construct worker URLs dynamically relative to client.js
    const sharedWorkerUrl = '/assets/scripts/v2/crack2-shared-worker.js';
    const dedicatedWorkerUrl = '/assets/scripts/v2/crack2-dedicated-worker.js';

    console.log(`[Client] Spawning Shared Worker from: ${sharedWorkerUrl}`);
    const sharedWorker = new SharedWorker(sharedWorkerUrl);

    let onMessageCallback = null;
    let pingSuccessful = false;
    let dedicatedWorker = null; // Broader scope to allow dynamic shutdown/termination

    // Start the communication port with the Shared Worker
    sharedWorker.port.start();

    // Helper to wait for a duration
    function sleep(ms) {
        return new Promise((resolve) => setTimeout(resolve, ms));
    }

    // Ping/pong check with timeout = 60ms, after fail sleep = 60ms, retry count = 10
    async function verifyConnection() {
        for (let attempt = 1; attempt <= 10; attempt++) {
            console.log(`[Client] Sending ping attempt ${attempt}/10...`);
            const pingId = Math.random().toString(36).substring(2);
            let pongReceived = false;

            const pingListener = (event) => {
                if (event.data && event.data.type === 'pong' && event.data.id === pingId) {
                    pongReceived = true;
                }
            };

            sharedWorker.port.addEventListener('message', pingListener);
            sharedWorker.port.postMessage({ type: 'ping', id: pingId });

            // Wait up to 60ms for pong
            await Promise.race([
                sleep(60),
                new Promise((resolve) => {
                    const checkInterval = setInterval(() => {
                        if (pongReceived) {
                            clearInterval(checkInterval);
                            resolve();
                        }
                    }, 2);
                })
            ]);

            // Clean up listener for this attempt
            sharedWorker.port.removeEventListener('message', pingListener);

            if (pongReceived) {
                console.log(`[Client] Pong received for attempt ${attempt}! Connection confirmed.`);
                pingSuccessful = true;
                break;
            } else {
                console.warn(`[Client] Ping attempt ${attempt} timed out. Sleeping 60ms before retry...`);
                await sleep(60);
            }
        }

        if (!pingSuccessful) {
            console.error('[Client] Failed to establish ping/pong connection with Shared Worker after 10 attempts.');
        }
    }

    // Execute verification loop for the Shared Worker
    verifyConnection();

    // Dedicated Worker spawning, direct ping/pong, initialization, and port bridging
    async function setupDedicatedWorker(url) {
        console.log('[Client] Setting up Dedicated Worker...');

        // In case one is already running, clean it up first
        if (dedicatedWorker) {
            try {
                dedicatedWorker.terminate();
            } catch (e) { }
            dedicatedWorker = null;
        }

        try {
            dedicatedWorker = new Worker(url);
        } catch (err) {
            console.error('[Client] Failed to construct Worker:', err);
            sharedWorker.port.postMessage({ type: 'DB_WORKER_INIT_FAILED', errorCode: 'CONSTRUCTION_FAILURE' });
            return;
        }

        // 1. Ping/pong loop with Dedicated Worker (timeout = 60ms, after fail sleep = 60ms, retry count = 10)
        let workerPingSucceeded = false;
        for (let attempt = 1; attempt <= 10; attempt++) {
            console.log(`[Client] Dedicated Worker direct ping attempt ${attempt}/10...`);
            const pingId = Math.random().toString(36).substring(2);
            let pongReceived = false;

            const listener = (event) => {
                if (event.data && event.data.type === 'pong' && event.data.id === pingId) {
                    pongReceived = true;
                }
            };

            dedicatedWorker.addEventListener('message', listener);
            dedicatedWorker.postMessage({ type: 'ping', id: pingId });

            // Wait up to 60ms for pong
            await Promise.race([
                sleep(60),
                new Promise((resolve) => {
                    const checkInterval = setInterval(() => {
                        if (pongReceived) {
                            clearInterval(checkInterval);
                            resolve();
                        }
                    }, 2);
                })
            ]);

            dedicatedWorker.removeEventListener('message', listener);

            if (pongReceived) {
                console.log(`[Client] Dedicated Worker direct pong received on attempt ${attempt}!`);
                workerPingSucceeded = true;
                break;
            } else {
                console.warn(`[Client] Dedicated Worker ping attempt ${attempt} timed out. Sleeping 60ms...`);
                await sleep(60);
            }
        }

        if (!workerPingSucceeded) {
            console.error('[Client] Dedicated Worker ping/pong failed after 10 attempts.');
            if (dedicatedWorker) {
                dedicatedWorker.terminate();
                dedicatedWorker = null;
            }
            sharedWorker.port.postMessage({ type: 'DB_WORKER_INIT_FAILED', errorCode: 'PING_FAILURE' });
            return;
        }

        // 2. Dedicated Worker actual initialization (one round of message)
        console.log('[Client] Dedicated Worker ping/pong succeeded. Proceeding to initialization...');
        let initCompleted = false;
        let initSucceeded = false;
        let initErrorMsg = '';

        const initListener = (event) => {
            if (event.data && event.data.type === 'init_result') {
                initCompleted = true;
                initSucceeded = event.data.success;
                if (!initSucceeded) {
                    initErrorMsg = event.data.error || 'Unknown initialization error';
                }
            }
        };

        dedicatedWorker.addEventListener('message', initListener);
        dedicatedWorker.postMessage({ type: 'init_dedicated_worker' });

        // Wait for the initialization response
        await new Promise((resolve) => {
            const checkInterval = setInterval(() => {
                if (initCompleted) {
                    clearInterval(checkInterval);
                    resolve();
                }
            }, 2);
        });

        dedicatedWorker.removeEventListener('message', initListener);

        if (!initSucceeded) {
            console.error(`[Client] Dedicated Worker initialization failed: ${initErrorMsg}`);
            if (dedicatedWorker) {
                dedicatedWorker.terminate();
                dedicatedWorker = null;
            }
            sharedWorker.port.postMessage({ type: 'DB_WORKER_INIT_FAILED', errorCode: 'INIT_FAILURE' });
            return;
        }

        console.log('[Client] Dedicated Worker initialization succeeded. Bridging ports...');

        // 3. Port Switching: Create MessageChannel and bridge
        const channel = new MessageChannel();
        dedicatedWorker.postMessage({ type: 'INIT_PORT' }, [channel.port1]);
        sharedWorker.port.postMessage({ type: 'REGISTER_DB_PORT' }, [channel.port2]);

        console.log('[Client] Dedicated Worker successfully bridged to Shared Worker.');
    }

    // Listen for control commands from the Shared Worker
    sharedWorker.port.addEventListener('message', (event) => {
        const data = event.data;
        if (!data) return;

        if (data.type === 'NEED_DB_WORKER') {
            console.log('[Client] Shared Worker requested a Dedicated Worker. Initializing setup sequence...');
            setupDedicatedWorker(dedicatedWorkerUrl);
        } else if (data.type === 'SHUTDOWN_WORKER') {
            console.log('[Client] Shared Worker requested shutdown of Dedicated Worker.');
            if (dedicatedWorker) {
                try {
                    dedicatedWorker.terminate();
                    console.log('[Client] Dedicated Worker terminated.');
                } catch (e) {
                    console.warn('[Client] Error terminating Dedicated Worker:', e);
                }
                dedicatedWorker = null;
            }
            sharedWorker.port.postMessage({ type: 'SHUTDOWN_OK' });
        } else if (data.type === 'forwarded_reply') {
            console.log('[Client] Received reply payload from Shared Worker:', data.payload, 'is_error:', data.is_error);
            if (typeof onMessageCallback === 'function') {
                onMessageCallback(data.payload, data.is_error);
            }
        }
    });

    // Handle page unload to notify Shared Worker for leader election / tab tracking
    if (typeof window !== 'undefined') {
        window.addEventListener('beforeunload', () => {
            console.log('[Client] Window is unloading. Notifying Shared Worker...');
            sharedWorker.port.postMessage({ type: 'unload' });
        });
    }

    // Return the standard WorkerHandles interface
    const handles = {
        send_message(message_obj) {
            console.log('[Client] WorkerHandles.send_message() called with:', message_obj);
            sharedWorker.port.postMessage({
                type: 'client_message',
                payload: message_obj
            });
        },
        set_onmessage(callback) {
            console.log('[Client] WorkerHandles.set_onmessage() registered callback.');
            onMessageCallback = callback;
        }
    };

    return handles;
}

console.log("[Client] exported function init_workers2.")
// Expose globally for browsers not using modules
if (typeof window !== 'undefined') {
    window.init_workers2 = init_workers2;
}
