

## Auto-generated signatures
<!-- Updated by gen-context.js -->
You are a coding assistant with complete knowledge of this codebase.
The following code signatures were extracted by SigMap v8.9.1 on 2026-07-06T12:44:48.373Z.
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
CLAUDE.md:18  # TODO: s
CLAUDE.md:20  # TODO: s
CLAUDE.md:21  # TODO: s
CLAUDE.md:22  # TODO: get which is missing...
CLAUDE.md:23  # TODO: get which is missing...
CLAUDE.md:24  # TODO: s
CLAUDE.md:25  # TODO: get which is missing...
CLAUDE.md:26  # TODO: s
CLAUDE.md:27  # TODO: get which is missing...
CLAUDE.md:28  # TODO: s
CLAUDE.md:29  # TODO: get which is missing...
CLAUDE.md:30  # TODO: get which is missing...
CLAUDE.md:31  # TODO: s
CLAUDE.md:32  # TODO: s
CLAUDE.md:33  # TODO: get which is missing...
CLAUDE.md:34  # TODO: get which is missing...
CLAUDE.md:35  # TODO: s
CLAUDE.md:36  # TODO: get which is missing...
CLAUDE.md:37  # TODO: s
CLAUDE.md:38  # TODO: get which is missing...
```

## .

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
