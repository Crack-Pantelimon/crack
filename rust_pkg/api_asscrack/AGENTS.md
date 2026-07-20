

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

## todos
```
src/crack_worker/api_worker.rs:43  # TODO: get which is missing...
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
