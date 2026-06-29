# Handoff Report: Codebase Exploration

This report compiles the findings on cargo builds, asset loading, player tracking, and database structures in this workspace.

## 1. Observation
Below are the exact files and lines observed during the investigation:

### Cargo Builds & Compilation
- **Root Workspace**: `/home/vasile/.gemini/antigravity/scratch/crack/Cargo.toml` contains members:
  ```toml
  members = [
      "packages/consensus_crackhead",
      "packages/net_crackpipe",
      "packages/storage_crackhouse",
      "packages/web_serviceworker_crackloader",
      "packages/web_serviceworker_crackslave",
      "packages/api_asscrack",
      "packages/thread_crackworker",
      "packages/_crack_utils",
      "crack_demo/web_worker",
      "crack_demo/thread_worker",
      "crack_demo/web_frontend",
      "crack_demo/demo_resolution_selector_web_bevy",
  ]
  ```
- **Native Platform Build Script**: `/home/vasile/.gemini/antigravity/scratch/crack/start_game_native.sh` contains:
  ```bash
  cd crack_demo/demo_resolution_selector_web_bevy
  cargo run
  ```
- **Web Platform Build Script**: `/home/vasile/.gemini/antigravity/scratch/crack/start_game_web.sh` contains:
  ```bash
  cd crack_demo/demo_resolution_selector_web_bevy
  trunk serve "$@"
  ```
- **Web Bundler Settings**: `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/Trunk.toml` configures Trunk build options (output directory `"dist"`, target `"index.html"`, features `["web"]`).

### Asset Loading in Bevy
- **Asset Base URL**: `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/config.rs` sets the asset source:
  ```rust
  #[cfg(feature = "web")]
  pub const DATA_BASE_URL: &str = "https://pantelimon.alt-f4.ro/";
  #[cfg(not(feature = "web"))]
  pub const DATA_BASE_URL: &str = "http://127.0.0.1:1973/";
  ```
- **Custom Parquet Asset Loader**: `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_metadata_parquet.rs` defines a custom asset loader for `.parquet` files:
  ```rust
  pub struct ParquetAssetLoader;
  impl AssetLoader for ParquetAssetLoader { ... }
  ```
  This is registered in `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/mod.rs` with:
  ```rust
  .init_asset_loader::<ParquetAssetLoader>()
  ```
- **GLB Models Load**: `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs` loads 3D scene files via `AssetServer::load`:
  ```rust
  let glb_url = format!("{}/3d_data/{}", crate::config::DATA_BASE_URL, filename);
  let asset_path = GltfAssetLabel::Scene(0).from_asset(glb_url);
  assets_and_handles.push((asset_id.clone(), asset_server.load(asset_path)));
  ```
- **Trunk Public Folder**: Static files like manifest, styling, and favicon reside in `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/public/` and are packaged by Trunk.

### Player / Car Tracking
- **Car Component**: `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/gta_plugin/car.rs` tracks the player's car using a marker component:
  ```rust
  #[derive(Component)]
  pub struct Car;
  ```
- **Physics Integration**: The car is spawned as a dynamic body using `avian3d`:
  ```rust
  commands.spawn((
      WorldAssetRoot(car_handle),
      Transform::from_translation(spawn_point + Vec3::new(0.0, 1.5, 0.0)),
      RigidBody::Dynamic,
      Collider::cuboid(2.0, 1.2, 4.5),
      LinearVelocity::default(),
      AngularVelocity::default(),
      Car,
  ));
  ```
- **System Access**: Systems query the car using Bevy query signatures:
  - Driving: `mut car_query: Query<(&Transform, &mut LinearVelocity, &mut AngularVelocity), With<Car>>`
  - Position clamping: `mut car_query: Query<(&mut Transform, &mut LinearVelocity), With<Car>>`
  - Camera follow: `car_query: Query<&Transform, With<Car>>`

### Storage & SQL Querying
- **Database Query APIs**: `/home/vasile/.gemini/antigravity/scratch/crack/packages/storage_crackhouse/src/api.rs` declares:
  ```rust
  pub async fn execute_sql2(sql: String) -> anyhow::Result<SqlResultSet> {
      crate::impl_rusqulite::sql_query(SQLAndParams { sql, params: vec![] }).await
  }

  pub async fn execute_sql_params(req: SQLAndParams) -> anyhow::Result<SqlResultSet> {
      crate::impl_rusqulite::sql_query(req).await
  }
  ```
- **SQLite Database Targets**: `/home/vasile/.gemini/antigravity/scratch/crack/packages/storage_crackhouse/src/impl_rusqulite.rs` opens connection on file target `post3.db` for native, and browser OPFS for WASM:
  ```rust
  #[cfg(all(target_family = "wasm", target_os = "unknown"))]
  const FILE: &str = "file:/assets/scripts/post3.db?vfs=opfs-sahpool";

  #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
  const FILE: &str = "post3.db";
  ```
- **Existing SQLite Databases**:
  - `post.db` schema:
    ```sql
    CREATE TABLE person (
        id    INTEGER PRIMARY KEY,
        name  TEXT NOT NULL,
        data  BLOB
    );
    ```
  - `post2.db` schema:
    ```sql
    CREATE TABLE "filling" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "name" varchar NOT NULL );
    CREATE TABLE "cake_filling" ( "cake_id" integer NOT NULL, "filling_id" integer NOT NULL, ... );
    CREATE TABLE "fruit" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "name" varchar NOT NULL, "cake_id" integer, ... );
    CREATE TABLE "cake" ( "id" integer NOT NULL PRIMARY KEY AUTOINCREMENT, "name" text );
    ```

---

## 2. Logic Chain
1. **Cargo Workspace and Bevy build options**: The root `Cargo.toml` workspace members and root scripts (`start_game_native.sh` & `start_game_web.sh`) show how the project is run across native (standard cargo run) and web (using trunk).
2. **Asset Loading mechanics**: Examining `config.rs` and `map_metadata_parquet.rs` shows that assets are requested dynamically via HTTP URLs from a `DATA_BASE_URL` rather than embedded locally, and custom file extensions (like `.parquet`) use custom `bevy::asset::AssetLoader` structs. Static client files (like manifest/CSS) live in `public/` and are copied to the build directory by Trunk.
3. **Player Car Tracking**: `car.rs` defines a struct `Car` marked as `#[derive(Component)]`. Bevy's ECS handles tracking the position of this entity using standard `Transform` and `avian3d` physics components (`LinearVelocity`/`AngularVelocity`), which systems query via standard component filters (e.g. `With<Car>`).
4. **SQL Querying Mechanism**: Checking `packages/storage_crackhouse/src/api.rs` and `impl_rusqulite.rs` proves that queries are run asynchronously against a local SQLite file (`post3.db` / OPFS target) using rusqlite via `execute_sql2` and `execute_sql_params`. We confirmed existing files `post.db` and `post2.db` contain schemas for `person` and `cake/filling` respectively via the sqlite3 CLI tool.

---

## 3. Caveats
- No actual `post3.db` database exists in the repository, suggesting it is created dynamically during runtime.
- The `PROJECT.md` specifies a future planned `MissionConfig` system using an SQLite table named `storage_crackhouse_MissionProgress` which does not yet exist in code or in the pre-existing databases (`post.db` and `post2.db`).
- JSON and YAML asset files are currently not loaded or parsed within Bevy in the codebase; their ingestion is planned for a future milestone.

---

## 4. Conclusion
The workspace comprises a dual-target Bevy setup compiled via standard cargo tools (native) and Trunk (web WASM). Assets are requested over HTTP and loaded dynamically, with custom types supported by custom `AssetLoader` structs. The player position is registered using a marker component `Car` in conjunction with `Transform` and `avian3d` velocity components. Database operations are routed via `storage_crackhouse` and execute queries on a local SQLite database, with existing reference tables housed in `post.db` and `post2.db`.

---

## 5. Verification Method
- **Verify Bevy Native Run**: Run `cargo run -p demo_resolution_selector_web_bevy` to check that the Bevy application compiles and executes natively.
- **Verify DB Queries**: Run the library tests in `packages/storage_crackhouse` to verify query execution and migrations:
  ```bash
  cargo test -p storage_crackhouse
  ```
- **Inspect DB Tables**: Use SQLite tools to inspect the structure of `post.db` and `post2.db`:
  ```bash
  sqlite3 post.db ".schema"
  sqlite3 post2.db ".schema"
  ```
