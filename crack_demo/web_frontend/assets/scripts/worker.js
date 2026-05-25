//#region: crack
let __wasm_worker_md5 = "1f938124b384490dfd895aeb13ba169a";  
console.log('__wasm_worker_md5 = ', __wasm_worker_md5)
//#endregion

try{


    importScripts('/assets/pkg_web_serviceworker/web_worker.js');

    console.log('Initializing worker')


    const {init_worker} = wasm_bindgen();

    console.log('init_worker fn ok:', init_worker)

    async function init_wasm_in_worker() {
        // Load the Wasm file by awaiting the Promise returned by `wasm_bindgen`.
        await wasm_bindgen('/assets/pkg_web_serviceworker/web_worker_bg.wasm');

        let worker = init_worker();
        console.log('init_worker done: ', worker);
        return worker;
    };

    init_wasm_in_worker();


    console.log("WORKER.JS : OK !")


}
catch (e) {
    console.log("WORKER.JS : FAILED !")
    console.error(e);

}
finally {
    console.log("WORKER.JS : FINISHED !")
}
