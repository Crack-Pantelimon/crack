

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
src/models.rs:182  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
```

## src

### src/api.rs
```
pub async fn execute_sql2(sql: String) → anyhow::Result<SqlResultSet>  :23-29
pub async fn execute_sql_params(req: SQLAndParams) → anyhow::Result<SqlResultSet>  :31-33
```

### src/impl_rusqulite.rs
```
pub async fn sql_query(sql: SQLAndParams) → anyhow::Result<SqlResultSet>  :23-61
```

### src/lib.rs
```
pub async fn install_opfs_sahpool() → anyhow::Result<()>  :7-17
pub async fn install_relaxed_idb() → anyhow::Result<()>  :20-30
```

### src/models.rs
```
pub struct ModelColumnImpl  :124-128
pub trait ModelGroup  :8-8
pub trait ModelDef  :22-22
pub trait ModelSerial  :29-29
pub trait DbTypeMapping  :202-205
impl i64  :207-210
impl String  :211-214
impl f64  :215-218
impl Vec  :219-222
impl Option  :223-226
pub async fn run_migrate_tables(groups: impl Iterator<Item = Arc<dyn ModelGroup>>,) → anyhow::Result<()>  :175-200
```

### src/types.rs
```
pub struct SQLAndParams  :4-7
pub struct SqlResultSet  :168-171
pub struct SqlResultRow  :174-176
pub enum DbValueType  :10-16
pub enum DbValue  :31-42
impl DbValueType  :18-28
  pub fn to_sql_str(&self) → &'static str  :19-19
impl DbValue  :43-50
  pub fn fold_option(value: Option<DbValue>) → DbValue  :44-44
impl TryFrom  :114-122
impl String  :125-125
impl i64  :126-126
impl f64  :127-127
impl Vec  :128-128
```
