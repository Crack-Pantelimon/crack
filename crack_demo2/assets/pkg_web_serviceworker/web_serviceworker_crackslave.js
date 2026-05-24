let wasm_bindgen = (function(exports) {
    let script_src;
    if (typeof document !== 'undefined' && document.currentScript !== null) {
        script_src = new URL(document.currentScript.src, location.href).toString();
    }

    //#region exports

    function init_worker() {
        wasm.init_worker();
    }
    exports.init_worker = init_worker;

    //#endregion

    //#region wasm imports
    function __wbg_get_imports() {
        const import0 = {
            __proto__: null,
            __wbg_Error_bce6d499ff0a4aff: function() { return logError(function (arg0, arg1) {
                const ret = Error(getStringFromWasm0(arg0, arg1));
                return ret;
            }, arguments); },
            __wbg_Number_b7972a139bfbfdf0: function() { return logError(function (arg0) {
                const ret = Number(arg0);
                return ret;
            }, arguments); },
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
            __wbg___wbindgen_in_07056af4f902c445: function(arg0, arg1) {
                const ret = arg0 in arg1;
                _assertBoolean(ret);
                return ret;
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
            __wbg___wbindgen_is_undefined_35bb9f4c7fd651d5: function(arg0) {
                const ret = arg0 === undefined;
                _assertBoolean(ret);
                return ret;
            },
            __wbg___wbindgen_jsval_loose_eq_0ad77b7717db155c: function(arg0, arg1) {
                const ret = arg0 == arg1;
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
            __wbg___wbindgen_rethrow_2b7cc655458909c2: function(arg0) {
                throw arg0;
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
            __wbg_call_13665d9f14390edc: function() { return handleError(function (arg0, arg1) {
                const ret = arg0.call(arg1);
                return ret;
            }, arguments); },
            __wbg_call_dfde26266607c996: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.call(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_claim_600468f38a68cee9: function() { return logError(function (arg0) {
                const ret = arg0.claim();
                return ret;
            }, arguments); },
            __wbg_clearTimeout_47a40e3be01ed7a3: function() { return handleError(function (arg0, arg1) {
                arg0.clearTimeout(arg1);
            }, arguments); },
            __wbg_clients_ef42f4172df00ccf: function() { return logError(function (arg0) {
                const ret = arg0.clients;
                return ret;
            }, arguments); },
            __wbg_data_b2194d058dbc32d3: function() { return logError(function (arg0) {
                const ret = arg0.data;
                return ret;
            }, arguments); },
            __wbg_done_54b8da57023b7ed2: function() { return logError(function (arg0) {
                const ret = arg0.done;
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_eval_32ea584b70eba338: function() { return handleError(function (arg0, arg1) {
                const ret = eval(getStringFromWasm0(arg0, arg1));
                return ret;
            }, arguments); },
            __wbg_getTime_09f1dd40a44edb30: function() { return logError(function (arg0) {
                const ret = arg0.getTime();
                return ret;
            }, arguments); },
            __wbg_get_3e9a707ab7d352eb: function() { return handleError(function (arg0, arg1) {
                const ret = Reflect.get(arg0, arg1);
                return ret;
            }, arguments); },
            __wbg_get_unchecked_1dfe6d05ad91d9b7: function() { return logError(function (arg0, arg1) {
                const ret = arg0[arg1 >>> 0];
                return ret;
            }, arguments); },
            __wbg_get_with_ref_key_6412cf3094599694: function() { return logError(function (arg0, arg1) {
                const ret = arg0[arg1];
                return ret;
            }, arguments); },
            __wbg_has_ef192b1f278770eb: function() { return handleError(function (arg0, arg1) {
                const ret = Reflect.has(arg0, arg1);
                _assertBoolean(ret);
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
            __wbg_instanceof_Uint8Array_abd07d4bd221d50b: function() { return logError(function (arg0) {
                let result;
                try {
                    result = arg0 instanceof Uint8Array;
                } catch (_) {
                    result = false;
                }
                const ret = result;
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_isArray_94898ed3aad6947b: function() { return logError(function (arg0) {
                const ret = Array.isArray(arg0);
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_isSafeInteger_01e964d144ad3a55: function() { return logError(function (arg0) {
                const ret = Number.isSafeInteger(arg0);
                _assertBoolean(ret);
                return ret;
            }, arguments); },
            __wbg_iterator_1441b47f341dc34f: function() { return logError(function () {
                const ret = Symbol.iterator;
                return ret;
            }, arguments); },
            __wbg_length_2591a0f4f659a55c: function() { return logError(function (arg0) {
                const ret = arg0.length;
                _assertNum(ret);
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
            __wbg_log_eb752234eec406d1: function() { return logError(function (arg0) {
                console.log(arg0);
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
            __wbg_new_02d162bc6cf02f60: function() { return logError(function () {
                const ret = new Object();
                return ret;
            }, arguments); },
            __wbg_new_0_2722fcdb71a888a6: function() { return logError(function () {
                const ret = new Date();
                return ret;
            }, arguments); },
            __wbg_new_310879b66b6e95e1: function() { return logError(function () {
                const ret = new Array();
                return ret;
            }, arguments); },
            __wbg_new_7ddec6de44ff8f5d: function() { return logError(function (arg0) {
                const ret = new Uint8Array(arg0);
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
            __wbg_now_e7c6795a7f81e10f: function() { return logError(function (arg0) {
                const ret = arg0.now();
                return ret;
            }, arguments); },
            __wbg_performance_3fcf6e32a7e1ed0a: function() { return logError(function (arg0) {
                const ret = arg0.performance;
                return ret;
            }, arguments); },
            __wbg_postMessage_5564134325e8942f: function() { return handleError(function (arg0, arg1) {
                arg0.postMessage(arg1);
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
            __wbg_registration_f8df4880ecd53ed2: function() { return logError(function (arg0) {
                const ret = arg0.registration;
                return ret;
            }, arguments); },
            __wbg_resolve_d17db9352f5a220e: function() { return logError(function (arg0) {
                const ret = Promise.resolve(arg0);
                return ret;
            }, arguments); },
            __wbg_run_be7c554bdaddf854: function() { return logError(function (arg0, arg1, arg2) {
                try {
                    var state0 = {a: arg1, b: arg2};
                    var cb0 = () => {
                        const a = state0.a;
                        state0.a = 0;
                        try {
                            return wasm_bindgen__convert__closures_____invoke__hb5bcbba6fe098490(a, state0.b, );
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
            __wbg_setTimeout_6613a51400c1bf9f: function() { return handleError(function (arg0, arg1, arg2) {
                const ret = arg0.setTimeout(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_set_6be42768c690e380: function() { return logError(function (arg0, arg1, arg2) {
                arg0[arg1] = arg2;
            }, arguments); },
            __wbg_set_78ea6a19f4818587: function() { return logError(function (arg0, arg1, arg2) {
                arg0[arg1 >>> 0] = arg2;
            }, arguments); },
            __wbg_set_onactivate_c4f79f26bdd3dcff: function() { return logError(function (arg0, arg1) {
                arg0.onactivate = arg1;
            }, arguments); },
            __wbg_set_oninstall_4f4a04d0ed97a87f: function() { return logError(function (arg0, arg1) {
                arg0.oninstall = arg1;
            }, arguments); },
            __wbg_set_onmessage_2658685ad16ce86e: function() { return logError(function (arg0, arg1) {
                arg0.onmessage = arg1;
            }, arguments); },
            __wbg_skipWaiting_59e1cc3753b08f32: function() { return handleError(function (arg0) {
                const ret = arg0.skipWaiting();
                return ret;
            }, arguments); },
            __wbg_source_6ba2c90b73df8d09: function() { return logError(function (arg0) {
                const ret = arg0.source;
                return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
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
            __wbg_then_837494e384b37459: function() { return logError(function (arg0, arg1) {
                const ret = arg0.then(arg1);
                return ret;
            }, arguments); },
            __wbg_then_bd927500e8905df2: function() { return logError(function (arg0, arg1, arg2) {
                const ret = arg0.then(arg1, arg2);
                return ret;
            }, arguments); },
            __wbg_update_e57e970c2e7e37cc: function() { return handleError(function (arg0) {
                const ret = arg0.update();
                return ret;
            }, arguments); },
            __wbg_value_9cc0518af87a489c: function() { return logError(function (arg0) {
                const ret = arg0.value;
                return ret;
            }, arguments); },
            __wbg_waitUntil_cdcf969450aa2fa9: function() { return handleError(function (arg0, arg1) {
                arg0.waitUntil(arg1);
            }, arguments); },
            __wbindgen_cast_0000000000000001: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [Externref], shim_idx: 249, ret: Result(Unit), inner_ret: Some(Result(Unit)) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__h71fc9d8a922bd63e);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000002: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("ExtendableEvent")], shim_idx: 10, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__h38c8fa325f11baa3);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000003: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [NamedExternref("ExtendableMessageEvent")], shim_idx: 12, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__h3df147ebae218a17);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000004: function() { return logError(function (arg0, arg1) {
                // Cast intrinsic for `Closure(Closure { owned: true, function: Function { arguments: [], shim_idx: 224, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
                const ret = makeMutClosure(arg0, arg1, wasm_bindgen__convert__closures_____invoke__h3c2004fb1d3f062a);
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000005: function() { return logError(function (arg0) {
                // Cast intrinsic for `F64 -> Externref`.
                const ret = arg0;
                return ret;
            }, arguments); },
            __wbindgen_cast_0000000000000006: function() { return logError(function (arg0, arg1) {
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
            "./web_serviceworker_crackslave_bg.js": import0,
        };
    }


    //#endregion
    function wasm_bindgen__convert__closures_____invoke__h3c2004fb1d3f062a(arg0, arg1) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__h3c2004fb1d3f062a(arg0, arg1);
    }

    function wasm_bindgen__convert__closures_____invoke__hb5bcbba6fe098490(arg0, arg1) {
        _assertNum(arg0);
        _assertNum(arg1);
        const ret = wasm.wasm_bindgen__convert__closures_____invoke__hb5bcbba6fe098490(arg0, arg1);
        return ret !== 0;
    }

    function wasm_bindgen__convert__closures_____invoke__h38c8fa325f11baa3(arg0, arg1, arg2) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__h38c8fa325f11baa3(arg0, arg1, arg2);
    }

    function wasm_bindgen__convert__closures_____invoke__h3df147ebae218a17(arg0, arg1, arg2) {
        _assertNum(arg0);
        _assertNum(arg1);
        wasm.wasm_bindgen__convert__closures_____invoke__h3df147ebae218a17(arg0, arg1, arg2);
    }

    function wasm_bindgen__convert__closures_____invoke__h71fc9d8a922bd63e(arg0, arg1, arg2) {
        _assertNum(arg0);
        _assertNum(arg1);
        const ret = wasm.wasm_bindgen__convert__closures_____invoke__h71fc9d8a922bd63e(arg0, arg1, arg2);
        if (ret[1]) {
            throw takeFromExternrefTable0(ret[0]);
        }
    }


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
let __wasm_script_md5 =   '55b78cac404836a5932021cfc329cc6f';
