//#region: crack
let __wasm_worker_md5 = "55b78cac404836a5932021cfc329cc6f";  
console.log('__wasm_worker_md5 = ', __wasm_worker_md5)
//#endregion

try{


    // The worker has its own scope and no direct access to functions/objects of the
    // global scope. We import the generated JS file to make `wasm_bindgen`
    // available which we need to initialize our Wasm code.
    importScripts('/assets/pkg_web_serviceworker/web_serviceworker_crackslave.js');

    console.log('Initializing worker')

    // In the worker, we have a different struct that we want to use as in
    // `index.js`.
    const {init_worker} = wasm_bindgen();






    console.log('init_worker fn ok:', init_worker)

    async function init_wasm_in_worker() {
        // Load the Wasm file by awaiting the Promise returned by `wasm_bindgen`.
        await wasm_bindgen('/assets/pkg_web_serviceworker/web_serviceworker_crackslave_bg.wasm');

        let worker = init_worker();
        console.log('init_worker done: ', worker);
        return worker;
    };

    init_wasm_in_worker();


}
catch (e) {
    console.log("WORKER.JS : FAILED !")
    console.error(e);

}
finally {
    console.log("WORKER.JS : FINISHED!")
}
