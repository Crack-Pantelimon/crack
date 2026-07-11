let wasm_bindgen = (function(exports) {
    let script_src;
    if (typeof document !== 'undefined' && document.currentScript !== null) {
        script_src = new URL(document.currentScript.src, location.href).toString();
    }

    //#region exports

    class IntoUnderlyingByteSource {
        constructor() {
            throw new Error('cannot invoke `new` directly');
        }
        __destroy_into_raw() {
            const ptr = this.__wbg_ptr;
            this.__wbg_ptr = 0;
            IntoUnderlyingByteSourceFinalization.unregister(this);
            return ptr;
        }
        free() {
            const ptr = this.__destroy_into_raw();
            wasm.__wbg_intounderlyingbytesource_free(ptr, 0);
        }
        /**
         * @returns {number}
         */
        get autoAllocateChunkSize() {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            _assertNum(this.__wbg_ptr);
            const ret = wasm.intounderlyingbytesource_autoAllocateChunkSize(this.__wbg_ptr);
            return ret >>> 0;
        }
        cancel() {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            const ptr = this.__destroy_into_raw();
            _assertNum(ptr);
            wasm.intounderlyingbytesource_cancel(ptr);
        }
        /**
         * @param {ReadableByteStreamController} controller
         * @returns {Promise<any>}
         */
        pull(controller) {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            _assertNum(this.__wbg_ptr);
            const ret = wasm.intounderlyingbytesource_pull(this.__wbg_ptr, controller);
            return ret;
        }
        /**
         * @param {ReadableByteStreamController} controller
         */
        start(controller) {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            _assertNum(this.__wbg_ptr);
            wasm.intounderlyingbytesource_start(this.__wbg_ptr, controller);
        }
        /**
         * @returns {ReadableStreamType}
         */
        get type() {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            _assertNum(this.__wbg_ptr);
            const ret = wasm.intounderlyingbytesource_type(this.__wbg_ptr);
            return __wbindgen_enum_ReadableStreamType[ret];
        }
    }
    if (Symbol.dispose) IntoUnderlyingByteSource.prototype[Symbol.dispose] = IntoUnderlyingByteSource.prototype.free;
    exports.IntoUnderlyingByteSource = IntoUnderlyingByteSource;

    class IntoUnderlyingSink {
        constructor() {
            throw new Error('cannot invoke `new` directly');
        }
        __destroy_into_raw() {
            const ptr = this.__wbg_ptr;
            this.__wbg_ptr = 0;
            IntoUnderlyingSinkFinalization.unregister(this);
            return ptr;
        }
        free() {
            const ptr = this.__destroy_into_raw();
            wasm.__wbg_intounderlyingsink_free(ptr, 0);
        }
        /**
         * @param {any} reason
         * @returns {Promise<any>}
         */
        abort(reason) {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            const ptr = this.__destroy_into_raw();
            _assertNum(ptr);
            const ret = wasm.intounderlyingsink_abort(ptr, reason);
            return ret;
        }
        /**
         * @returns {Promise<any>}
         */
        close() {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            const ptr = this.__destroy_into_raw();
            _assertNum(ptr);
            const ret = wasm.intounderlyingsink_close(ptr);
            return ret;
        }
        /**
         * @param {any} chunk
         * @returns {Promise<any>}
         */
        write(chunk) {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            _assertNum(this.__wbg_ptr);
            const ret = wasm.intounderlyingsink_write(this.__wbg_ptr, chunk);
            return ret;
        }
    }
    if (Symbol.dispose) IntoUnderlyingSink.prototype[Symbol.dispose] = IntoUnderlyingSink.prototype.free;
    exports.IntoUnderlyingSink = IntoUnderlyingSink;

    class IntoUnderlyingSource {
        constructor() {
            throw new Error('cannot invoke `new` directly');
        }
        __destroy_into_raw() {
            const ptr = this.__wbg_ptr;
            this.__wbg_ptr = 0;
            IntoUnderlyingSourceFinalization.unregister(this);
            return ptr;
        }
        free() {
            const ptr = this.__destroy_into_raw();
            wasm.__wbg_intounderlyingsource_free(ptr, 0);
        }
        cancel() {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            const ptr = this.__destroy_into_raw();
            _assertNum(ptr);
            wasm.intounderlyingsource_cancel(ptr);
        }
        /**
         * @param {ReadableStreamDefaultController} controller
         * @returns {Promise<any>}
         */
        pull(controller) {
            if (this.__wbg_ptr == 0) throw new Error('Attempt to use a moved value');
            _assertNum(this.__wbg_ptr);
            const ret = wasm.intounderlyingsource_pull(this.__wbg_ptr, controller);
            return ret;
        }
    }
    if (Symbol.dispose) IntoUnderlyingSource.prototype[Symbol.dispose] = IntoUnderlyingSource.prototype.free;
    exports.IntoUnderlyingSource = IntoUnderlyingSource;

    /**
     * @param {any} msg
     * @returns {Promise<any>}
     */
    function computePayloadReply(msg) {
        const ret = wasm.computePayloadReply(msg);
        return ret;
    }
    exports.computePayloadReply = computePayloadReply;

    /**
     * @returns {Promise<void>}
     */
    function initDedicatedWorker() {
        const ret = wasm.initDedicatedWorker();
        return ret;
    }
    exports.initDedicatedWorker = initDedicatedWorker;

    function init_worker() {
        wasm.init_worker();
    }
    exports.init_worker = init_worker;

    //#endregion

    //#region wasm imports
    function __wbg_get_imports() {
        const import0 = {
            __proto__: null,
            __wbg___wbindgen_boolean_get_2304fb8c853028c8: function(arg0) {
                const v = arg0;
                const ret = typeof(v) === 'boolean' ? v : undefined;
                if (!isLikeNone(ret)) {
                    _assertBoolean(ret);
                }
                return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
            },
            __wbg___wbindgen_debug_string_edece8177ad01481: function(arg0, arg1) {
                const ret = debugString(arg1);
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg___wbindgen_is_function_5cd60d5cf78b4eef: function(arg0) {
                const ret = typeof(arg0) === 'function';
                _assertBoolean(ret);
                return ret;
            },
            __wbg___wbindgen_is_object_b4593df85baada48: function(arg0) {
                const val = arg0;
                const ret = typeof(val) === 'object' && val !== null;
                _assertBoolean(ret);
                return ret;
            },
            __wbg___wbindgen_is_string_dde0fd9020db4434: function(arg0) {
                const ret = typeof(arg0) === 'string';
                _assertBoolean(ret);
                return ret;
            },
            __wbg___wbindgen_is_undefined_35bb9f4c7fd651d5: function(arg0) {
                const ret = arg0 === undefined;
                _assertBoolean(ret);
                return ret;
            },
            __wbg___wbindgen_number_get_f73a1244370fcc2c: function(arg0, arg1) {
                const obj = arg1;
                const ret = typeof(obj) === 'number' ? obj : undefined;
                if (!isLikeNone(ret)) {
                    _assertNum(ret);
                }
                getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
            },
            __wbg___wbindgen_string_get_d109740c0d18f4d7: function(arg0, arg1) {
                const obj = arg1;
                const ret = typeof(obj) === 'string' ? obj : undefined;
                var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                var len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            },
            __wbg___wbindgen_throw_9c31b086c2b26051: function(arg0, arg1) {
                throw new Error(getStringFromWasm0(arg0, arg1));
            },
            __wbg__wbg_cb_unref_3fa391f3fcdb55f8: function() { return logError(function (arg0) {
                arg0._wbg_cb_unref();
            }, arguments); },
            __wbg_abort_89f7368e16055f5f: function() { return logError(function (arg0, arg1) {
                arg0.abort(arg1);
            }, arguments); },
            __wbg_abort_b363e6285472a358: function() { return logError(function (arg0) {
                arg0.abort();
            }, arguments); },
            __wbg_addEventListener_6f3a037944400810: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3);
            }, arguments); },
            __wbg_append_263958599fd198c1: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
                arg0.append(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
            }, arguments); },
            __wbg_arrayBuffer_cb5d4748b5f3cad5: function() { return handleError(function (arg0) {
                const ret = arg0.arrayBuffer();
                return ret;
            }, arguments); },
            __wbg_body_acbb5ec9cd18657f: function() { return logError(function (arg0) {
                const ret = arg0.body;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            }, arguments); },
            __wbg_buffer_8d6798e32d1afd34: function() { return logError(function (arg0) {
                const ret = arg0.buffer;
                return ret;
            }, arguments); },
            __wbg_byobRequest_9d8c3b7b2f692560: function() { return logError(function (arg0) {
                const ret = arg0.byobRequest;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            }, arguments); },
            __wbg_byteLength_c0cecdd68fab1693: function() { return logError(function (arg0) {
                const ret = arg0.byteLength;
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_byteOffset_3791b0030cc3b490: function() { return logError(function (arg0) {
                const ret = arg0.byteOffset;
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_call_13665d9f14390edc: function() { return handleError(function (arg0, arg1) {
                const ret = arg0.call(arg1);
                return ret;
            }, arguments); },
            __wbg_call_dfde26266607c996: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.call(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_cancel_bee68d5707c614fb: function() { return logError(function (arg0) {
                const ret = arg0.cancel();
                return ret;
            }, arguments); },
            __wbg_catch_ec5061a695c26496: function() { return logError(function (arg0, arg1) {
                const ret = arg0.catch(arg1);
                return ret;
            }, arguments); },
            __wbg_clearTimeout_3437408b3800a087: function() { return handleError(function (arg0, arg1) {
                arg0.clearTimeout(arg1);
            }, arguments); },
            __wbg_clearTimeout_47a40e3be01ed7a3: function() { return handleError(function (arg0, arg1) {
                arg0.clearTimeout(arg1);
            }, arguments); },
            __wbg_clearTimeout_6b8d9a38b9263d65: function() { return logError(function (arg0) {
                const ret = clearTimeout(arg0);
                return ret;
            }, arguments); },
            __wbg_close_53179a3d37ed525d: function() { return handleError(function (arg0) {
                arg0.close();
            }, arguments); },
            __wbg_close_807d553ef8405788: function() { return handleError(function (arg0) {
                arg0.close();
            }, arguments); },
            __wbg_close_e1f28ed6c57d2b8b: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                arg0.close(arg1, getStringFromWasm0(arg2, arg3));
            }, arguments); },
            __wbg_close_e323e9eee669c291: function() { return handleError(function (arg0) {
                arg0.close();
            }, arguments); },
            __wbg_code_98ceeaa5ff83fb0b: function() { return logError(function (arg0) {
                const ret = arg0.code;
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_createSyncAccessHandle_b43931d1786c8893: function() { return logError(function (arg0) {
                const ret = arg0.createSyncAccessHandle();
                return ret;
            }, arguments); },
            __wbg_crypto_38df2bab126b63dc: function() { return logError(function (arg0) {
                const ret = arg0.crypto;
                return ret;
            }, arguments); },
            __wbg_data_5fc79a19e47d1531: function() { return logError(function (arg0) {
                const ret = arg0.data;
                return ret;
            }, arguments); },
            __wbg_done_54b8da57023b7ed2: function() { return logError(function (arg0) {
                const ret = arg0.done;
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_enqueue_c3ce0a986a355a8c: function() { return handleError(function (arg0, arg1) {
                arg0.enqueue(arg1);
            }, arguments); },
            __wbg_entries_8db5a11d802d4c53: function() { return logError(function (arg0) {
                const ret = arg0.entries();
                return ret;
            }, arguments); },
            __wbg_fetch_2998af8c54e0997c: function() { return logError(function (arg0, arg1) {
                const ret = arg0.fetch(arg1);
                return ret;
            }, arguments); },
            __wbg_fetch_9dad4fe911207b37: function() { return logError(function (arg0) {
                const ret = fetch(arg0);
                return ret;
            }, arguments); },
            __wbg_fill_353d1cfad8f7aaba: function() { return logError(function (arg0, arg1, arg2, arg3) {
                const ret = arg0.fill(arg1, arg2 >>> 0, arg3 >>> 0);
                return ret;
            }, arguments); },
            __wbg_flush_6e0326e516db3bdd: function() { return handleError(function (arg0) {
                arg0.flush();
            }, arguments); },
            __wbg_getDate_a52123c8affc9072: function() { return logError(function (arg0) {
                const ret = arg0.getDate();
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_getDay_50a9ee1e4d17dc24: function() { return logError(function (arg0) {
                const ret = arg0.getDay();
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_getDirectoryHandle_ad42f2fe4fcbaa50: function() { return logError(function (arg0, arg1, arg2, arg3) {
                const ret = arg0.getDirectoryHandle(getStringFromWasm0(arg1, arg2), arg3);
                return ret;
            }, arguments); },
            __wbg_getDirectory_1992c7a67af9adfe: function() { return logError(function (arg0) {
                const ret = arg0.getDirectory();
                return ret;
            }, arguments); },
            __wbg_getFileHandle_4b28d8702a1efafd: function() { return logError(function (arg0, arg1, arg2, arg3) {
                const ret = arg0.getFileHandle(getStringFromWasm0(arg1, arg2), arg3);
                return ret;
            }, arguments); },
            __wbg_getFullYear_d5d1f7de344fdc5b: function() { return logError(function (arg0) {
                const ret = arg0.getFullYear();
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_getHours_c974d920209733e8: function() { return logError(function (arg0) {
                const ret = arg0.getHours();
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_getMinutes_e2e8ae846b37b328: function() { return logError(function (arg0) {
                const ret = arg0.getMinutes();
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_getMonth_de70091920053153: function() { return logError(function (arg0) {
                const ret = arg0.getMonth();
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_getRandomValues_3f44b700395062e5: function() { return handleError(function (arg0, arg1) {
                globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
            }, arguments); },
            __wbg_getRandomValues_8c4660d7727bfad0: function() { return handleError(function (arg0, arg1) {
                globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
            }, arguments); },
            __wbg_getRandomValues_a697888e9ba1eee3: function() { return handleError(function (arg0, arg1) {
                globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
            }, arguments); },
            __wbg_getRandomValues_c44a50d8cfdaebeb: function() { return handleError(function (arg0, arg1) {
                arg0.getRandomValues(arg1);
            }, arguments); },
            __wbg_getReader_b4b1868fbca77dbe: function() { return handleError(function (arg0) {
                const ret = arg0.getReader();
                return ret;
            }, arguments); },
            __wbg_getSeconds_2782a558f414ec05: function() { return logError(function (arg0) {
                const ret = arg0.getSeconds();
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_getSize_d908d6c50ab44407: function() { return handleError(function (arg0) {
                const ret = arg0.getSize();
                return ret;
            }, arguments); },
            __wbg_getTime_09f1dd40a44edb30: function() { return logError(function (arg0) {
                const ret = arg0.getTime();
                return ret;
            }, arguments); },
            __wbg_getTimezoneOffset_96cfb6ddebc9e5ca: function() { return logError(function (arg0) {
                const ret = arg0.getTimezoneOffset();
                return ret;
            }, arguments); },
            __wbg_getUint32_276edcdc13dc6029: function() { return logError(function (arg0, arg1) {
                const ret = arg0.getUint32(arg1 >>> 0);
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_get_3e9a707ab7d352eb: function() { return handleError(function (arg0, arg1) {
                const ret = Reflect.get(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_get_98fdf51d029a75eb: function() { return logError(function (arg0, arg1) {
                const ret = arg0[arg1 >>> 0];
                return ret;
            }, arguments); },
            __wbg_get_dcf82ab8aad1a593: function() { return handleError(function (arg0, arg1) {
                const ret = Reflect.get(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_get_done_06210bfbda89c407: function() { return logError(function (arg0) {
                const ret = arg0.done;
                if (!isLikeNone(ret)) {
                    _assertBoolean(ret);
                }
                return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
            }, arguments); },
            __wbg_get_index_c051becca25aa6d8: function() { return logError(function (arg0, arg1) {
                const ret = arg0[arg1 >>> 0];
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_get_value_31eb9abef97d98cb: function() { return logError(function (arg0) {
                const ret = arg0.value;
                return ret;
            }, arguments); },
            __wbg_has_ef192b1f278770eb: function() { return handleError(function (arg0, arg1) {
                const ret = Reflect.has(arg0, arg1);
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_headers_18f39f24d3837dc1: function() { return logError(function (arg0) {
                const ret = arg0.headers;
                return ret;
            }, arguments); },
            __wbg_instanceof_ArrayBuffer_53db37b06f6b9afe: function() { return logError(function (arg0) {
                let result;
                try {
                    result = arg0 instanceof ArrayBuffer;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_instanceof_Blob_ef93d187bbde5360: function() { return logError(function (arg0) {
                let result;
                try {
                    result = arg0 instanceof Blob;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_instanceof_Response_ecfc823e8fb354e2: function() { return logError(function (arg0) {
                let result;
                try {
                    result = arg0 instanceof Response;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_instanceof_WorkerGlobalScope_a93ee1765e6a23bf: function() { return logError(function (arg0) {
                let result;
                try {
                    result = arg0 instanceof WorkerGlobalScope;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_iterator_1441b47f341dc34f: function() { return logError(function () {
                const ret = Symbol.iterator;
                return ret;
            }, arguments); },
            __wbg_length_56fcd3e2b7e0299d: function() { return logError(function (arg0) {
                const ret = arg0.length;
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_log_0c201ade58bb55e1: function() { return logError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7) {
                let deferred0_0;
                let deferred0_1;
                try {
                    deferred0_0 = arg0;
                    deferred0_1 = arg1;
                    console.log(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3), getStringFromWasm0(arg4, arg5), getStringFromWasm0(arg6, arg7));
                } finally {
                    wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
                }
            }, arguments); },
            __wbg_log_ce2c4456b290c5e7: function() { return logError(function (arg0, arg1) {
                let deferred0_0;
                let deferred0_1;
                try {
                    deferred0_0 = arg0;
                    deferred0_1 = arg1;
                    console.log(getStringFromWasm0(arg0, arg1));
                } finally {
                    wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
                }
            }, arguments); },
            __wbg_mark_b4d943f3bc2d2404: function() { return logError(function (arg0, arg1) {
                performance.mark(getStringFromWasm0(arg0, arg1));
            }, arguments); },
            __wbg_measure_84362959e621a2c1: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                let deferred0_0;
                let deferred0_1;
                let deferred1_0;
                let deferred1_1;
                try {
                    deferred0_0 = arg0;
                    deferred0_1 = arg1;
                    deferred1_0 = arg2;
                    deferred1_1 = arg3;
                    performance.measure(getStringFromWasm0(arg0, arg1), getStringFromWasm0(arg2, arg3));
                } finally {
                    wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
                    wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
                }
            }, arguments); },
            __wbg_msCrypto_bd5a034af96bcba6: function() { return logError(function (arg0) {
                const ret = arg0.msCrypto;
                return ret;
            }, arguments); },
            __wbg_navigator_3334c390f542c642: function() { return logError(function (arg0) {
                const ret = arg0.navigator;
                return ret;
            }, arguments); },
            __wbg_new_02d162bc6cf02f60: function() { return logError(function () {
                const ret = new Object();
                return ret;
            }, arguments); },
            __wbg_new_0_2722fcdb71a888a6: function() { return logError(function () {
                const ret = new Date();
                return ret;
            }, arguments); },
            __wbg_new_1f236d63ba0c4784: function() { return logError(function (arg0, arg1) {
                const ret = new Error(getStringFromWasm0(arg0, arg1));
                return ret;
            }, arguments); },
            __wbg_new_69045bcc0a85f678: function() { return logError(function (arg0, arg1, arg2) {
                const ret = new DataView(arg0, arg1 >>> 0, arg2 >>> 0);
                return ret;
            }, arguments); },
            __wbg_new_7ddec6de44ff8f5d: function() { return logError(function (arg0) {
                const ret = new Uint8Array(arg0);
                return ret;
            }, arguments); },
            __wbg_new_859b9002e2668e82: function() { return logError(function (arg0) {
                const ret = new Date(arg0);
                return ret;
            }, arguments); },
            __wbg_new_af86d8f14640f1f3: function() { return handleError(function () {
                const ret = new AbortController();
                return ret;
            }, arguments); },
            __wbg_new_b1280f836646084c: function() { return handleError(function (arg0, arg1) {
                const ret = new WebSocket(getStringFromWasm0(arg0, arg1));
                return ret;
            }, arguments); },
            __wbg_new_ee0be486d8f01282: function() { return handleError(function () {
                const ret = new Headers();
                return ret;
            }, arguments); },
            __wbg_new_from_slice_269e35316ed2d061: function() { return logError(function (arg0, arg1) {
                const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
                return ret;
            }, arguments); },
            __wbg_new_typed_c072c4ce9a2a0cdf: function() { return logError(function (arg0, arg1) {
                try {
                    var state0 = {a: arg0, b: arg1};
                    var cb0 = (arg0, arg1) => {
                        const a = state0.a;
                        state0.a = 0;
                        try {
                            return wasm_bindgen__convert__closures_____invoke__h9b2cb83b40122b7c(a, state0.b, arg0, arg1);
                        } finally {
                            state0.a = a;
                        }
                    };
                    const ret = new Promise(cb0);
                    return ret;
                } finally {
                    state0.a = 0;
                }
            }, arguments); },
            __wbg_new_with_byte_offset_and_length_a87e79143162d67f: function() { return logError(function (arg0, arg1, arg2) {
                const ret = new Uint8Array(arg0, arg1 >>> 0, arg2 >>> 0);
                return ret;
            }, arguments); },
            __wbg_new_with_length_99887c91eae4abab: function() { return logError(function (arg0) {
                const ret = new Uint8Array(arg0 >>> 0);
                return ret;
            }, arguments); },
            __wbg_new_with_str_and_init_ffe9977c986ea039: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = new Request(getStringFromWasm0(arg0, arg1), arg2);
                return ret;
            }, arguments); },
            __wbg_new_with_year_month_day_0ccdc1cc3a42b726: function() { return logError(function (arg0, arg1, arg2) {
                const ret = new Date(arg0 >>> 0, arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_next_2a4e19f4f5083b0f: function() { return logError(function (arg0) {
                const ret = arg0.next;
                return ret;
            }, arguments); },
            __wbg_next_6429a146bf756f93: function() { return handleError(function (arg0) {
                const ret = arg0.next();
                return ret;
            }, arguments); },
            __wbg_next_ef3752486079ef35: function() { return handleError(function (arg0) {
                const ret = arg0.next();
                return ret;
            }, arguments); },
            __wbg_node_84ea875411254db1: function() { return logError(function (arg0) {
                const ret = arg0.node;
                return ret;
            }, arguments); },
            __wbg_now_81363d44c96dd239: function() { return logError(function () {
                const ret = Date.now();
                return ret;
            }, arguments); },
            __wbg_now_e7c6795a7f81e10f: function() { return logError(function (arg0) {
                const ret = arg0.now();
                return ret;
            }, arguments); },
            __wbg_performance_3fcf6e32a7e1ed0a: function() { return logError(function (arg0) {
                const ret = arg0.performance;
                return ret;
            }, arguments); },
            __wbg_process_44c7a14e11e9f69e: function() { return logError(function (arg0) {
                const ret = arg0.process;
                return ret;
            }, arguments); },
            __wbg_prototypesetcall_5f9bdc8d75e07276: function() { return logError(function (arg0, arg1, arg2) {
                Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
            }, arguments); },
            __wbg_queueMicrotask_78d584b53af520f5: function() { return logError(function (arg0) {
                const ret = arg0.queueMicrotask;
                return ret;
            }, arguments); },
            __wbg_queueMicrotask_b39ea83c7f01971a: function() { return logError(function (arg0) {
                queueMicrotask(arg0);
            }, arguments); },
            __wbg_randomFillSync_6c25eac9869eb53c: function() { return handleError(function (arg0, arg1) {
                arg0.randomFillSync(arg1);
            }, arguments); },
            __wbg_random_a8dfe52b70cb65a5: function() { return logError(function () {
                const ret = Math.random();
                return ret;
            }, arguments); },
            __wbg_read_254bf22401498310: function() { return logError(function (arg0) {
                const ret = arg0.read();
                return ret;
            }, arguments); },
            __wbg_read_9cdf4eb589f4d559: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.read(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_read_e98a66da7c0a9f60: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                const ret = arg0.read(getArrayU8FromWasm0(arg1, arg2), arg3);
                return ret;
            }, arguments); },
            __wbg_readyState_a1a00cc8898812ac: function() { return logError(function (arg0) {
                const ret = arg0.readyState;
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_reason_48e6f2ed86d09534: function() { return logError(function (arg0, arg1) {
                const ret = arg1.reason;
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            }, arguments); },
            __wbg_releaseLock_65f356509fef84ac: function() { return logError(function (arg0) {
                arg0.releaseLock();
            }, arguments); },
            __wbg_removeEventListener_74e3ac8e219bcdc7: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                arg0.removeEventListener(getStringFromWasm0(arg1, arg2), arg3);
            }, arguments); },
            __wbg_require_b4edbdcf3e2a1ef0: function() { return handleError(function () {
                const ret = module.require;
                return ret;
            }, arguments); },
            __wbg_resolve_d17db9352f5a220e: function() { return logError(function (arg0) {
                const ret = Promise.resolve(arg0);
                return ret;
            }, arguments); },
            __wbg_respond_0196e052b003e1db: function() { return handleError(function (arg0, arg1) {
                arg0.respond(arg1 >>> 0);
            }, arguments); },
            __wbg_run_be7c554bdaddf854: function() { return logError(function (arg0, arg1, arg2) {
                try {
                    var state0 = {a: arg1, b: arg2};
                    var cb0 = () => {
                        const a = state0.a;
                        state0.a = 0;
                        try {
                            return wasm_bindgen__convert__closures_____invoke__h7cdfead19564aa99(a, state0.b, );
                        } finally {
                            state0.a = a;
                        }
                    };
                    const ret = arg0.run(cb0);
                    _assertBoolean(ret);
                    return ret;
                } finally {
                    state0.a = 0;
                }
            }, arguments); },
            __wbg_send_b9d998a91cbc8429: function() { return handleError(function (arg0, arg1, arg2) {
                arg0.send(getArrayU8FromWasm0(arg1, arg2));
            }, arguments); },
            __wbg_send_e1d2f71ce4473d1e: function() { return handleError(function (arg0, arg1, arg2) {
                arg0.send(getStringFromWasm0(arg1, arg2));
            }, arguments); },
            __wbg_setTimeout_6613a51400c1bf9f: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.setTimeout(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_setTimeout_c3233ba3a378f224: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.setTimeout(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_setTimeout_f757f00851f76c42: function() { return logError(function (arg0, arg1) {
                const ret = setTimeout(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_setUint32_372e483df43c3c71: function() { return logError(function (arg0, arg1, arg2) {
                arg0.setUint32(arg1 >>> 0, arg2 >>> 0);
            }, arguments); },
            __wbg_set_24d0fa9e104112f9: function() { return logError(function (arg0, arg1, arg2) {
                arg0.set(getArrayU8FromWasm0(arg1, arg2));
            }, arguments); },
            __wbg_set_a0e911be3da02782: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = Reflect.set(arg0, arg1, arg2);
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_set_at_5bf8e61b318bb1b6: function() { return logError(function (arg0, arg1) {
                arg0.at = arg1;
            }, arguments); },
            __wbg_set_binaryType_5c0002dfcf194934: function() { return logError(function (arg0, arg1) {
                arg0.binaryType = __wbindgen_enum_BinaryType[arg1];
            }, arguments); },
            __wbg_set_body_7f56457720e81672: function() { return logError(function (arg0, arg1) {
                arg0.body = arg1;
            }, arguments); },
            __wbg_set_cache_9ed01a3813d96de2: function() { return logError(function (arg0, arg1) {
                arg0.cache = __wbindgen_enum_RequestCache[arg1];
            }, arguments); },
            __wbg_set_create_5cb26b898b874123: function() { return logError(function (arg0, arg1) {
                arg0.create = arg1 !== 0;
            }, arguments); },
            __wbg_set_create_d6d52ca457b8e339: function() { return logError(function (arg0, arg1) {
                arg0.create = arg1 !== 0;
            }, arguments); },
            __wbg_set_credentials_55b92faec8dcc6a4: function() { return logError(function (arg0, arg1) {
                arg0.credentials = __wbindgen_enum_RequestCredentials[arg1];
            }, arguments); },
            __wbg_set_handle_event_6471b5e5fe16e12f: function() { return logError(function (arg0, arg1) {
                arg0.handleEvent = arg1;
            }, arguments); },
            __wbg_set_headers_97ed66619adb1e3e: function() { return logError(function (arg0, arg1) {
                arg0.headers = arg1;
            }, arguments); },
            __wbg_set_method_4d69a1a7e34c0aca: function() { return logError(function (arg0, arg1, arg2) {
                arg0.method = getStringFromWasm0(arg1, arg2);
            }, arguments); },
            __wbg_set_mode_dfc59bbbe25b1d14: function() { return logError(function (arg0, arg1) {
                arg0.mode = __wbindgen_enum_RequestMode[arg1];
            }, arguments); },
            __wbg_set_onclose_3121e15055418a37: function() { return logError(function (arg0, arg1) {
                arg0.onclose = arg1;
            }, arguments); },
            __wbg_set_onerror_1dd52df6279c8dd2: function() { return logError(function (arg0, arg1) {
                arg0.onerror = arg1;
            }, arguments); },
            __wbg_set_onmessage_9eb2cf76e70783ad: function() { return logError(function (arg0, arg1) {
                arg0.onmessage = arg1;
            }, arguments); },
            __wbg_set_onopen_6f3fc5e2ad3144f1: function() { return logError(function (arg0, arg1) {
                arg0.onopen = arg1;
            }, arguments); },
            __wbg_set_signal_2a5bd3615938edbc: function() { return logError(function (arg0, arg1) {
                arg0.signal = arg1;
            }, arguments); },
            __wbg_signal_304beac95c8c5ea0: function() { return logError(function (arg0) {
                const ret = arg0.signal;
                return ret;
            }, arguments); },
            __wbg_slice_a4396daa26ff759e: function() { return logError(function (arg0, arg1, arg2) {
                const ret = arg0.slice(arg1 >>> 0, arg2 >>> 0);
                return ret;
            }, arguments); },
            __wbg_static_accessor_CREATE_TASK_b0b1bf7dd60e5453: function() { return logError(function () {
                const ret = typeof console === 'undefined' ? null : console?.createTask;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            }, arguments); },
            __wbg_static_accessor_GLOBAL_THIS_02344c9b09eb08a9: function() { return logError(function () {
                const ret = typeof globalThis === 'undefined' ? null : globalThis;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            }, arguments); },
            __wbg_static_accessor_GLOBAL_ac6d4ac874d5cd54: function() { return logError(function () {
                const ret = typeof global === 'undefined' ? null : global;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            }, arguments); },
            __wbg_static_accessor_SELF_9b2406c23aeb2023: function() { return logError(function () {
                const ret = typeof self === 'undefined' ? null : self;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            }, arguments); },
            __wbg_static_accessor_WINDOW_b34d2126934e16ba: function() { return logError(function () {
                const ret = typeof window === 'undefined' ? null : window;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            }, arguments); },
            __wbg_status_0853c9f5752c7ee2: function() { return logError(function (arg0) {
                const ret = arg0.status;
                _assertNum(ret);
                return ret;
            }, arguments); },
            __wbg_storage_bf5c84c54465ffc3: function() { return logError(function (arg0) {
                const ret = arg0.storage;
                return ret;
            }, arguments); },
            __wbg_stringify_ef0c105b1ccc3849: function() { return handleError(function (arg0) {
                const ret = JSON.stringify(arg0);
                return ret;
            }, arguments); },
            __wbg_subarray_7c6a0da8f3b4a1ba: function() { return logError(function (arg0, arg1, arg2) {
                const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
                return ret;
            }, arguments); },
            __wbg_text_99930d92d5f1b540: function() { return handleError(function (arg0) {
                const ret = arg0.text();
                return ret;
            }, arguments); },
            __wbg_then_837494e384b37459: function() { return logError(function (arg0, arg1) {
                const ret = arg0.then(arg1);
                return ret;
            }, arguments); },
            __wbg_then_bd927500e8905df2: function() { return logError(function (arg0, arg1, arg2) {
                const ret = arg0.then(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_truncate_35560bd680646378: function() { return handleError(function (arg0, arg1) {
                arg0.truncate(arg1);
            }, arguments); },
            __wbg_truncate_791f1cb608d22a2e: function() { return handleError(function (arg0, arg1) {
                arg0.truncate(arg1 >>> 0);
            }, arguments); },
            __wbg_url_1a5ea6a8a7f22ff8: function() { return logError(function (arg0, arg1) {
                const ret = arg1.url;
                const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
                const len1 = WASM_VECTOR_LEN;
                getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
                getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
            }, arguments); },
            __wbg_value_9cc0518af87a489c: function() { return logError(function (arg0) {
                const ret = arg0.value;
                return ret;
            }, arguments); },
            __wbg_versions_276b2795b1c6a219: function() { return logError(function (arg0) {
                const ret = arg0.versions;
                return ret;
            }, arguments); },
            __wbg_view_1b637c097280508c: function() { return logError(function (arg0) {
                const ret = arg0.view;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
            }, arguments); },
            __wbg_write_0dcbb9f6678b32f9: function() { return handleError(function (arg0, arg1, arg2, arg3) {
                const ret = arg0.write(getArrayU8FromWasm0(arg1, arg2), arg3);
                return ret;
            }, arguments); },
            __wbg_write_60db26d2060142fb: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.write(arg1, arg2);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000001: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 4376, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__hbdddea019499929e);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000002: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 6189, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__hec3756495c0351d9);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000003: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("CloseEvent")], shim_idx: 5873, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000004: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("ErrorEvent")], shim_idx: 5873, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52_3);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000005: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("Event")], shim_idx: 5873, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52_4);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000006: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 5873, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52_5);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000007: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 4341, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__hf179b7c5bf04c244);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000008: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 4486, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
                const ret = makeClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__hae81952207a8e993);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000009: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 5933, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__ha9bd23f76d2e14f3);
                return ret;
            }, arguments); },
            __wbindgen_cast_000000000000000a: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 6155, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__h61ffb8517521dd73);
                return ret;
            }, arguments); },
            __wbindgen_cast_000000000000000b: function() { return logError(function (arg0) {
                // Cast intrinsic for `F64 -> Externref`.
                const ret = arg0;
                return ret;
            }, arguments); },
            __wbindgen_cast_000000000000000c: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
                const ret = getArrayU8FromWasm0(arg0, arg1);
                return ret;
            }, arguments); },
            __wbindgen_cast_000000000000000d: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Ref(String) -> Externref`.
                const ret = getStringFromWasm0(arg0, arg1);
                return ret;
            }, arguments); },
            __wbindgen_init_externref_table: function() {
                const table = wasm.__wbindgen_externrefs;
                const offset = table.grow(4);
                table.set(0, undefined);
                table.set(offset + 0, undefined);
                table.set(offset + 1, null);
                table.set(offset + 2, true);
                table.set(offset + 3, false);
            },
        };
        return {
            __proto__: null,
            "./web_worker_bg.js": import0,
        };
    }


    //#endregion
    function wasm_bindgen__convert__closures_____invoke__hf179b7c5bf04c244(arg0, arg1) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__hf179b7c5bf04c244(arg0, arg1);
    }

    function wasm_bindgen__convert__closures_____invoke__hae81952207a8e993(arg0, arg1) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__hae81952207a8e993(arg0, arg1);
    }

    function wasm_bindgen__convert__closures_____invoke__ha9bd23f76d2e14f3(arg0, arg1) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__ha9bd23f76d2e14f3(arg0, arg1);
    }

    function wasm_bindgen__convert__closures_____invoke__h61ffb8517521dd73(arg0, arg1) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__h61ffb8517521dd73(arg0, arg1);
    }

    function wasm_bindgen__convert__closures_____invoke__h7cdfead19564aa99(arg0, arg1) {
        _assertNum(arg0);
        _assertNum(arg1);
        const ret = wasm.wasm_bindgen__convert__closures_____invoke__h7cdfead19564aa99(arg0, arg1);
        return ret !== 0;
    }

    function wasm_bindgen__convert__closures_____invoke__hbdddea019499929e(arg0, arg1, arg2) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__hbdddea019499929e(arg0, arg1, arg2);
    }

    function wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52(arg0, arg1, arg2) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52(arg0, arg1, arg2);
    }

    function wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52_3(arg0, arg1, arg2) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52_3(arg0, arg1, arg2);
    }

    function wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52_4(arg0, arg1, arg2) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52_4(arg0, arg1, arg2);
    }

    function wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52_5(arg0, arg1, arg2) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__h0b10084200f80a52_5(arg0, arg1, arg2);
    }

    function wasm_bindgen__convert__closures_____invoke__hec3756495c0351d9(arg0, arg1, arg2) {
        _assertNum(arg0);
        _assertNum(arg1);
        const ret = wasm.wasm_bindgen__convert__closures_____invoke__hec3756495c0351d9(arg0, arg1, arg2);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }

    function wasm_bindgen__convert__closures_____invoke__h9b2cb83b40122b7c(arg0, arg1, arg2, arg3) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__h9b2cb83b40122b7c(arg0, arg1, arg2, arg3);
    }


    const __wbindgen_enum_BinaryType = ["blob", "arraybuffer"];


    const __wbindgen_enum_ReadableStreamType = ["bytes"];


    const __wbindgen_enum_RequestCache = ["default", "no-store", "reload", "no-cache", "force-cache", "only-if-cached"];


    const __wbindgen_enum_RequestCredentials = ["omit", "same-origin", "include"];


    const __wbindgen_enum_RequestMode = ["same-origin", "no-cors", "cors", "navigate"];
    const IntoUnderlyingByteSourceFinalization = (typeof FinalizationRegistry === 'undefined')
        ? { register: () => {}, unregister: () => {} }
        : new FinalizationRegistry(ptr => wasm.__wbg_intounderlyingbytesource_free(ptr, 1));
    const IntoUnderlyingSinkFinalization = (typeof FinalizationRegistry === 'undefined')
        ? { register: () => {}, unregister: () => {} }
        : new FinalizationRegistry(ptr => wasm.__wbg_intounderlyingsink_free(ptr, 1));
    const IntoUnderlyingSourceFinalization = (typeof FinalizationRegistry === 'undefined')
        ? { register: () => {}, unregister: () => {} }
        : new FinalizationRegistry(ptr => wasm.__wbg_intounderlyingsource_free(ptr, 1));


    //#region intrinsics
    function addToExternrefTable0(obj) {
        const idx = wasm.__externref_table_alloc();
        wasm.__wbindgen_externrefs.set(idx, obj);
        return idx;
    }

    function _assertBoolean(n) {
        if (typeof(n) !== 'boolean') {
            throw new Error(`expected a boolean argument, found ${typeof(n)}`);
        }
    }

    function _assertNum(n) {
        if (typeof(n) !== 'number') throw new Error(`expected a number argument, found ${typeof(n)}`);
    }

    const CLOSURE_DTORS = (typeof FinalizationRegistry === 'undefined')
        ? { register: () => {}, unregister: () => {} }
        : new FinalizationRegistry(state => wasm.__wbindgen_destroy_closure(state.a, state.b));

    function debugString(val) {
        // primitive types
        const type = typeof val;
        if (type == 'number' || type == 'boolean' || val == null) {
            return  `${val}`;
        }
        if (type == 'string') {
            return `"${val}"`;
        }
        if (type == 'symbol') {
            const description = val.description;
            if (description == null) {
                return 'Symbol';
            } else {
                return `Symbol(${description})`;
            }
        }
        if (type == 'function') {
            const name = val.name;
            if (typeof name == 'string' && name.length > 0) {
                return `Function(${name})`;
            } else {
                return 'Function';
            }
        }
        // objects
        if (Array.isArray(val)) {
            const length = val.length;
            let debug = '[';
            if (length > 0) {
                debug += debugString(val[0]);
            }
            for(let i = 1; i < length; i++) {
                debug += ', ' + debugString(val[i]);
            }
            debug += ']';
            return debug;
        }
        // Test for built-in
        const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
        let className;
        if (builtInMatches && builtInMatches.length > 1) {
            className = builtInMatches[1];
        } else {
            // Failed to match the standard '[object ClassName]'
            return toString.call(val);
        }
        if (className == 'Object') {
            // we're a user defined class or Object
            // JSON.stringify avoids problems with cycles, and is generally much
            // easier than looping through ownProperties of `val`.
            try {
                return 'Object(' + JSON.stringify(val) + ')';
            } catch (_) {
                return 'Object';
            }
        }
        // errors
        if (val instanceof Error) {
            return `${val.name}: ${val.message}\n${val.stack}`;
        }
        // TODO we could test for more things here, like `Set`s and `Map`s.
        return className;
    }

    function getArrayU8FromWasm0(ptr, len) {
        ptr = ptr >>> 0;
        return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
    }

    let cachedDataViewMemory0 = null;
    function getDataViewMemory0() {
        if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
            cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
        }
        return cachedDataViewMemory0;
    }

    function getStringFromWasm0(ptr, len) {
        return decodeText(ptr >>> 0, len);
    }

    let cachedUint8ArrayMemory0 = null;
    function getUint8ArrayMemory0() {
        if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
            cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
        }
        return cachedUint8ArrayMemory0;
    }

    function handleError(f, args) {
        try {
            return f.apply(this, args);
        } catch (e) {
            const idx = addToExternrefTable0(e);
            wasm.__wbindgen_exn_store(idx);
        }
    }

    function isLikeNone(x) {
        return x === undefined || x === null;
    }

    function logError(f, args) {
        try {
            return f.apply(this, args);
        } catch (e) {
            let error = (function () {
                try {
                    return e instanceof Error ? `${e.message}\n\nStack:\n${e.stack}` : e.toString();
                } catch(_) {
                    return "<failed to stringify thrown value>";
                }
            }());
            console.error("wasm-bindgen: imported JS function that was not marked as `catch` threw an error:", error);
            throw e;
        }
    }

    function makeClosure(arg0, arg1, f) {
        const state = { a: arg0, b: arg1, cnt: 1 };
        const real = (...args) => {

            // First up with a closure we increment the internal reference
            // count. This ensures that the Rust closure environment won't
            // be deallocated while we're invoking it.
            state.cnt++;
            try {
                return f(state.a, state.b, ...args);
            } finally {
                real._wbg_cb_unref();
            }
        };
        real._wbg_cb_unref = () => {
            if (--state.cnt === 0) {
                wasm.__wbindgen_destroy_closure(state.a, state.b);
                state.a = 0;
                CLOSURE_DTORS.unregister(state);
            }
        };
        CLOSURE_DTORS.register(real, state, state);
        return real;
    }

    function makeMutClosure(arg0, arg1, f) {
        const state = { a: arg0, b: arg1, cnt: 1 };
        const real = (...args) => {

            // First up with a closure we increment the internal reference
            // count. This ensures that the Rust closure environment won't
            // be deallocated while we're invoking it.
            state.cnt++;
            const a = state.a;
            state.a = 0;
            try {
                return f(a, state.b, ...args);
            } finally {
                state.a = a;
                real._wbg_cb_unref();
            }
        };
        real._wbg_cb_unref = () => {
            if (--state.cnt === 0) {
                wasm.__wbindgen_destroy_closure(state.a, state.b);
                state.a = 0;
                CLOSURE_DTORS.unregister(state);
            }
        };
        CLOSURE_DTORS.register(real, state, state);
        return real;
    }

    function passStringToWasm0(arg, malloc, realloc) {
        if (typeof(arg) !== 'string') throw new Error(`expected a string argument, found ${typeof(arg)}`);
        if (realloc === undefined) {
            const buf = cachedTextEncoder.encode(arg);
            const ptr = malloc(buf.length, 1) >>> 0;
            getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
            WASM_VECTOR_LEN = buf.length;
            return ptr;
        }

        let len = arg.length;
        let ptr = malloc(len, 1) >>> 0;

        const mem = getUint8ArrayMemory0();

        let offset = 0;

        for (; offset < len; offset++) {
            const code = arg.charCodeAt(offset);
            if (code > 0x7F) break;
            mem[ptr + offset] = code;
        }
        if (offset !== len) {
            if (offset !== 0) {
                arg = arg.slice(offset);
            }
            ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
            const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
            const ret = cachedTextEncoder.encodeInto(arg, view);
            if (ret.read !== arg.length) throw new Error('failed to pass whole string');
            offset += ret.written;
            ptr = realloc(ptr, len, offset, 1) >>> 0;
        }

        WASM_VECTOR_LEN = offset;
        return ptr;
    }

    function takeFromExternrefTable0(idx) {
        const value = wasm.__wbindgen_externrefs.get(idx);
        wasm.__externref_table_dealloc(idx);
        return value;
    }

    let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
    cachedTextDecoder.decode();
    function decodeText(ptr, len) {
        return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
    }

    const cachedTextEncoder = new TextEncoder();

    if (!('encodeInto' in cachedTextEncoder)) {
        cachedTextEncoder.encodeInto = function (arg, view) {
            const buf = cachedTextEncoder.encode(arg);
            view.set(buf);
            return {
                read: arg.length,
                written: buf.length
            };
        };
    }

    let WASM_VECTOR_LEN = 0;


    //#endregion

    //#region wasm loading
    let wasmModule, wasmInstance, wasm;
    function __wbg_finalize_init(instance, module) {
        wasmInstance = instance;
        wasm = instance.exports;
        wasmModule = module;
        cachedDataViewMemory0 = null;
        cachedUint8ArrayMemory0 = null;
        wasm.__wbindgen_start();
        return wasm;
    }

    async function __wbg_load(module, imports) {
        if (typeof Response === 'function' && module instanceof Response) {
            if (typeof WebAssembly.instantiateStreaming === 'function') {
                try {
                    return await WebAssembly.instantiateStreaming(module, imports);
                } catch (e) {
                    const validResponse = module.ok && expectedResponseType(module.type);

                    if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                        console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                    } else { throw e; }
                }
            }

            const bytes = await module.arrayBuffer();
            return await WebAssembly.instantiate(bytes, imports);
        } else {
            const instance = await WebAssembly.instantiate(module, imports);

            if (instance instanceof WebAssembly.Instance) {
                return { instance, module };
            } else {
                return instance;
            }
        }

        function expectedResponseType(type) {
            switch (type) {
                case 'basic': case 'cors': case 'default': return true;
            }
            return false;
        }
    }

    function initSync(module) {
        if (wasm !== undefined) return wasm;


        if (module !== undefined) {
            if (Object.getPrototypeOf(module) === Object.prototype) {
                ({module} = module)
            } else {
                console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
            }
        }

        const imports = __wbg_get_imports();
        if (!(module instanceof WebAssembly.Module)) {
            module = new WebAssembly.Module(module);
        }
        const instance = new WebAssembly.Instance(module, imports);
        return __wbg_finalize_init(instance, module);
    }

    async function __wbg_init(module_or_path) {
        if (wasm !== undefined) return wasm;


        if (module_or_path !== undefined) {
            if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
                ({module_or_path} = module_or_path)
            } else {
                console.warn('using deprecated parameters for the initialization function; pass a single object instead')
            }
        }

        if (module_or_path === undefined && script_src !== undefined) {
            module_or_path = script_src.replace(/\.js$/, "_bg.wasm");
        }
        const imports = __wbg_get_imports();

        if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
            module_or_path = fetch(module_or_path);
        }

        const { instance, module } = await __wbg_load(await module_or_path, imports);

        return __wbg_finalize_init(instance, module);
    }

    //#endregion
    exports.__wasm = wasm;
    return Object.assign(__wbg_init, { initSync }, exports);
})({ __proto__: null });
//#region: crack
let __wasm_script_md5 =   'a7e464abf5a0e7c6ac5f1fcae00f8127';
