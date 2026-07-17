

## Auto-generated signatures
<!-- Updated by gen-context.js -->
# Code signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## src

### src/lib.rs
```
pub async fn _js_init_dedicated_worker() → Result<(), JsValue>  :13-28
pub async fn _js_compute_payload_reply(msg: JsValue) → Result<JsValue, JsValue>  :31-37
pub async fn web_worker_registration(mapping: Arc<ApiImplMapping>,) → std::result::Result<(), JsV...  :121-132
```
