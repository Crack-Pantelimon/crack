

## Auto-generated signatures
<!-- Updated by gen-context.js -->
You are a coding assistant with complete knowledge of this codebase.
The following code signatures were extracted by SigMap v8.9.1 on 2026-07-06T12:44:48.350Z.
<!-- sigmap: version=8.9.1 -->

These signatures represent every public function, class, and type in the project.
Refer to them when answering questions about code structure, APIs, and implementation.
## Code Signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## .

### CLAUDE.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 CLAUDE.md
h3 AGENTS.md
h3 Cargo.toml
h2 .github
h3 .github/copilot-instructions.md
h3 .github/gemini-context.md
h2 src
h3 src/lib.rs
code-fence plain
```

### AGENTS.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 CLAUDE.md
h3 AGENTS.md
h3 Cargo.toml
h2 .github
h3 .github/copilot-instructions.md
h3 .github/gemini-context.md
h2 src
h3 src/lib.rs
code-fence plain
```

### Cargo.toml
```
table [package]
table [dependencies]
table [target.'cfg(target_family = "wasm")'.dependencies]
table [target.'cfg(not(target_family = "wasm"))'.dependencies]
table [lints]
table [features]
key name
key version.workspace
key authors.workspace
key edition.workspace
key n0-future
key rand
key getrandom
key tokio.workspace
key workspace
```

## .github

### .github/copilot-instructions.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 CLAUDE.md
h3 AGENTS.md
h3 Cargo.toml
h2 .github
h3 .github/copilot-instructions.md
h3 .github/gemini-context.md
h2 src
h3 src/lib.rs
code-fence plain
```

### .github/gemini-context.md
```
h2 Auto-generated signatures
h2 Code Signatures
h2 SigMap commands
h2 .
h3 CLAUDE.md
h3 AGENTS.md
h3 Cargo.toml
h2 .github
h3 .github/copilot-instructions.md
h3 .github/gemini-context.md
h2 src
h3 src/lib.rs
code-fence plain
```

## src

### src/lib.rs
```
pub fn get_timestamp_now_ms() → i64
pub fn spawn(f: F) → n0_future::task::JoinHandle...
pub fn random_u32() → u32
pub async fn sleep_ms(dt_ms: u32)
```
