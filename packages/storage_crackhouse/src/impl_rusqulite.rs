use rusqlite::{Connection, Result};

#[derive(Debug)]
struct Person {
    id: i32,
    name: String,
    data: Option<Vec<u8>>,
}

pub fn connect() -> Result<Connection> {

    // ON WASM
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    const FILE: &str = "file:/assets/scripts/post.db?vfs=opfs-sahpool";

    // ON NON-WASM
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    const FILE: &str = "post.db";

    Connection::open(FILE)
}


pub fn run_test_person() -> Result<()> {

        let conn = connect()?;
        tracing::info!("CREATE TABLE");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS person (
            id    INTEGER PRIMARY KEY,
            name  TEXT NOT NULL,
            data  BLOB
        )",
        (), // empty list of parameters.
    )?;
    let me = Person {
        id: 0,
        name: "Steven".to_string(),
        data: None,
    };
    conn.execute(
        "INSERT INTO person (name, data) VALUES (?1, ?2)",
        (&me.name, &me.data),
    )?;

    let mut stmt = conn.prepare("SELECT id, name, data FROM person")?;
    let person_iter = stmt.query_map([], |row| {
        Ok(Person {
            id: row.get(0)?,
            name: row.get(1)?,
            data: row.get(2)?,
        })
    })?;

    for person in person_iter {
        tracing::info!("Found person {}: {:?}", person.as_ref().unwrap().id, person.as_ref().unwrap());
    }
    Ok(())
}

pub fn sql_inject(sql: String) -> anyhow::Result<String> {
    let conn = connect()?;
    let mut _stmt = conn.prepare(&sql)?;
    let mut _resp = _stmt.raw_query();
    let mut txt = String::from("");
    while let Ok(Some(_row)) = _resp.next() {
        let rowtxt = format!("{_row:?}\n\n");
        txt += &rowtxt;
    }

    Ok(txt)
}