

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
pub fn get_timestamp_now_ms() → i64
pub fn spawn(f: F) → n0_future::task::JoinHandle...
pub fn random_u32() → u32
pub async fn sleep_ms(dt_ms: u32)
```
