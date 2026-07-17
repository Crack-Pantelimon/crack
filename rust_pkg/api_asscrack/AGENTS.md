

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
pub struct ApiClient  :11-16
pub struct MessageLater  :22-24
impl ApiClient  :46-105
  pub fn new(pipe: WorkerPipe) → Self  :47-47
  pub async fn call(&self, arg: T::Arg) → anyhow::Result<T::Ret>  :63-63
```

### src/api/api_method_macros.rs
```
pub struct ApiGroupDeclStatic  :16-18
pub struct ApiMethodInfo  :96-101
pub struct ApiMethodImpl  :104-109
pub trait ApiGroupDecl  :3-5
pub trait ApiGroupMethods  :6-9
pub trait ApiGroupImpls  :11-11
pub trait ApiMethodDecl  :20-93
impl ApiMethodImpl  :111-117
  pub fn fullname(&self) → String  :112-112
impl ApiMethodInfo  :119-125
  pub fn fullname(&self) → String  :120-120
```

### src/api/api_worker_declarations.rs
```
pub async fn worker_ping(_x: () → anyhow::Result<()>  :20-23
```

### src/crack_worker/api_worker.rs
```
pub struct ApiImplMapping  :11-14
pub fn make_api_mapping(groups: Vec<Arc<dyn ApiGroupImpls>>) → Arc<ApiImplMapping>  :16-75
pub async fn compute_response_message(_request: WorkerMessage, mapping: Arc<ApiImplMapping>,) → WorkerMessage  :87-101
```

### src/crack_worker/mod.rs
```
pub struct WorkerPipe  :3-6
pub struct WorkerMessage  :9-13
pub trait WorkerLoaderFactory  :16-18
```
