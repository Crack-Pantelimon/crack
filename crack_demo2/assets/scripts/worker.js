// The worker has its own scope and no direct access to functions/objects of the
// global scope. We import the generated JS file to make `wasm_bindgen`
// available which we need to initialize our Wasm code.
importScripts('/assets/pkg_web_serviceworker/web_serviceworker_crackslave.js');

console.log('Initializing worker')

// In the worker, we have a different struct that we want to use as in
// `index.js`.
const {init_worker} = wasm_bindgen();




//#region: crack
let __wasm_worker_md5 = "06190d0e013a8c20a6692e180a94c8c4";  
//#endregion

console.log('init_worker fn ok:', init_worker)

async function init_wasm_in_worker() {
    // Load the Wasm file by awaiting the Promise returned by `wasm_bindgen`.
    await wasm_bindgen('/assets/pkg_web_serviceworker/web_serviceworker_crackslave_bg.wasm');

    let worker = init_worker();
    console.log('init_worker done: ', worker);
    return worker;
};

init_wasm_in_worker();

