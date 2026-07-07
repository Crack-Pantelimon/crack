

## Auto-generated signatures
<!-- Updated by gen-context.js -->
You are a coding assistant with complete knowledge of this codebase.
The following code signatures were extracted by SigMap v8.9.1 on 2026-07-07T13:11:21.057Z.
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
.github/copilot-instructions.md:27  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
.github/copilot-instructions.md:28  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
.github/copilot-instructions.md:29  # TODO: s
.github/copilot-instructions.md:30  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
.github/copilot-instructions.md:31  # TODO: s
.github/copilot-instructions.md:32  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
.github/copilot-instructions.md:33  # TODO: s
.github/copilot-instructions.md:34  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
.github/copilot-instructions.md:35  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
.github/copilot-instructions.md:36  # TODO: s
.github/copilot-instructions.md:37  # TODO: s
.github/copilot-instructions.md:38  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
.github/copilot-instructions.md:39  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
.github/copilot-instructions.md:40  # TODO: s
.github/copilot-instructions.md:41  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
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
h3 src/api.rs
h3 src/impl_rusqulite.rs
h3 src/lib.rs
h3 src/models.rs
h3 src/types.rs
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
h3 src/api.rs
h3 src/impl_rusqulite.rs
h3 src/lib.rs
h3 src/models.rs
h3 src/types.rs
code-fence plain
```

### Cargo.toml
```
table [package]
table [dependencies]
table [target.'cfg(all(target_family = "wasm", target_os = "unknown"))'.dependencies]
key name
key version.workspace
key authors.workspace
key edition.workspace
key tracing.workspace
key serde_json
key anyhow.workspace
key serde-wasm-bindgen
key wasm-bindgen
key wasm-bindgen-futures
key lazy_static
key sqlite-wasm-vfs
key sqlite-wasm-rs
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
h3 src/api.rs
h3 src/impl_rusqulite.rs
h3 src/lib.rs
h3 src/models.rs
h3 src/types.rs
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
h3 src/api.rs
h3 src/impl_rusqulite.rs
h3 src/lib.rs
h3 src/models.rs
h3 src/types.rs
code-fence plain
```

## src

### src/api.rs
```
pub async fn execute_sql2(sql: String) → anyhow::Result<SqlResultSet>
pub async fn execute_sql_params(req: SQLAndParams) → anyhow::Result<SqlResultSet>
```

### src/impl_rusqulite.rs
```
pub async fn sql_query(sql: SQLAndParams) → anyhow::Result<SqlResultSet>
```

### src/lib.rs
```
pub async fn install_opfs_sahpool() → anyhow::Result<()>
pub async fn install_relaxed_idb() → anyhow::Result<()>
```

### src/models.rs
```
pub struct ModelColumnImpl
pub trait ModelGroup
pub trait ModelDef
pub trait ModelSerial
pub trait DbTypeMapping
impl i64
impl String
impl f64
impl Vec
impl Option
pub async fn run_migrate_tables(groups: impl Iterator<Item = Arc<dyn ModelGroup>>,) → anyhow::Result<()>
```

### src/types.rs
```
pub struct SQLAndParams
pub struct SqlResultSet
pub struct SqlResultRow
pub enum DbValueType
pub enum DbValue
impl DbValueType
  pub fn to_sql_str(&self) → &'static str
impl DbValue
  pub fn fold_option(value: Option<DbValue>) → DbValue
impl TryFrom
impl String
impl i64
impl f64
impl Vec
```
