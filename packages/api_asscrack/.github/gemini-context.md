

## Auto-generated signatures
<!-- Updated by gen-context.js -->
You are a coding assistant with complete knowledge of this codebase.
The following code signatures were extracted by SigMap v8.9.1 on 2026-07-07T13:11:20.985Z.
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

## todos
```
.github/copilot-instructions.md:22  # TODO: s
.github/copilot-instructions.md:24  # TODO: s
.github/copilot-instructions.md:25  # TODO: s
.github/copilot-instructions.md:26  # TODO: s
.github/copilot-instructions.md:27  # TODO: get which is missing...
.github/copilot-instructions.md:28  # TODO: get which is missing...
.github/copilot-instructions.md:29  # TODO: s
.github/copilot-instructions.md:30  # TODO: get which is missing...
.github/copilot-instructions.md:31  # TODO: s
.github/copilot-instructions.md:32  # TODO: get which is missing...
.github/copilot-instructions.md:33  # TODO: s
.github/copilot-instructions.md:34  # TODO: get which is missing...
.github/copilot-instructions.md:35  # TODO: get which is missing...
.github/copilot-instructions.md:36  # TODO: s
.github/copilot-instructions.md:37  # TODO: s
.github/copilot-instructions.md:38  # TODO: get which is missing...
.github/copilot-instructions.md:39  # TODO: get which is missing...
.github/copilot-instructions.md:40  # TODO: s
.github/copilot-instructions.md:41  # TODO: get which is missing...
.github/copilot-instructions.md:42  # TODO: s
```

## .

### AGENTS.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 todos
h2 .
h3 CLAUDE.md
h3 AGENTS.md
h3 Cargo.toml
h2 .github
h3 .github/copilot-instructions.md
h3 .github/gemini-context.md
h2 src
h3 src/api/api_client.rs
h3 src/api/api_method_macros.rs
h3 src/api/api_worker_declarations.rs
h3 src/crack_worker/api_worker.rs
h3 src/crack_worker/mod.rs
code-fence plain
```

### CLAUDE.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 todos
h2 .
h3 CLAUDE.md
h3 AGENTS.md
h3 Cargo.toml
h2 .github
h3 .github/copilot-instructions.md
h3 .github/gemini-context.md
h2 src
h3 src/api/api_client.rs
h3 src/api/api_method_macros.rs
h3 src/api/api_worker_declarations.rs
h3 src/crack_worker/api_worker.rs
h3 src/crack_worker/mod.rs
code-fence plain
```

### Cargo.toml
```
table [package]
table [dependencies]
table [lints]
key name
key version.workspace
key authors.workspace
key edition.workspace
key serde.workspace
key tracing.workspace
key anyhow.workspace
key async-trait
key paste
key futures
key workspace
```

## .github

### .github/copilot-instructions.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 todos
h2 .
h3 CLAUDE.md
h3 AGENTS.md
h3 Cargo.toml
h2 .github
h3 .github/copilot-instructions.md
h3 .github/gemini-context.md
h2 src
h3 src/api/api_client.rs
h3 src/api/api_method_macros.rs
h3 src/api/api_worker_declarations.rs
h3 src/crack_worker/api_worker.rs
h3 src/crack_worker/mod.rs
code-fence plain
```

### .github/gemini-context.md
```
h2 Auto-generated signatures
h2 Code Signatures
h2 SigMap commands
h2 todos
h2 .
h3 CLAUDE.md
h3 AGENTS.md
h3 Cargo.toml
h2 .github
h3 .github/copilot-instructions.md
h3 .github/gemini-context.md
h2 src
h3 src/api/api_client.rs
h3 src/api/api_method_macros.rs
h3 src/api/api_worker_declarations.rs
h3 src/crack_worker/api_worker.rs
h3 src/crack_worker/mod.rs
code-fence plain
```

## src

### src/api/api_client.rs
```
pub struct ApiClient
pub struct MessageLater
impl ApiClient
  pub fn new(pipe: WorkerPipe) → Self
  pub async fn call(&self, arg: T::Arg) → anyhow::Result<T::Ret>
```

### src/api/api_method_macros.rs
```
pub struct ApiGroupDeclStatic
pub struct ApiMethodInfo
pub struct ApiMethodImpl
pub trait ApiGroupDecl
pub trait ApiGroupMethods
pub trait ApiGroupImpls
pub trait ApiMethodDecl
impl ApiMethodImpl
  pub fn fullname(&self) → String
impl ApiMethodInfo
  pub fn fullname(&self) → String
```

### src/api/api_worker_declarations.rs
```
pub async fn worker_ping(_x: () → anyhow::Result<()>
```

### src/crack_worker/api_worker.rs
```
pub struct ApiImplMapping
pub fn make_api_mapping(groups: Vec<Arc<dyn ApiGroupImpls>>) → Arc<ApiImplMapping>
pub async fn compute_response_message(_request: WorkerMessage, mapping: Arc<ApiImplMapping>,) → WorkerMessage
```

### src/crack_worker/mod.rs
```
pub struct WorkerPipe
pub struct WorkerMessage
pub trait WorkerLoaderFactory
```
