use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

use rusqlite::{Connection, params};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn main() {
    let output = PathBuf::from(env::args_os().nth(1).expect("output path"));
    let database = output.with_extension("anki21");
    let connection = Connection::open(&database).expect("open fixture database");
    connection
        .execute_batch(
            "CREATE TABLE col (ver INTEGER NOT NULL, models TEXT NOT NULL);\n\
             CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, flds BLOB NOT NULL);\n\
             CREATE TABLE cards (id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, ord INTEGER NOT NULL, type INTEGER NOT NULL, queue INTEGER NOT NULL, due INTEGER NOT NULL, ivl INTEGER NOT NULL, factor INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL);",
        )
        .expect("fixture schema");
    let models = serde_json::json!({
        "1": {
            "type": 0,
            "flds": [{"name": "Front", "ord": 0}, {"name": "Back", "ord": 1}],
            "tmpls": [{
                "name": "Card 1",
                "ord": 0,
                "qfmt": "{{Front}}",
                "afmt": "{{FrontSide}}<hr id=answer>{{Back}}"
            }],
            "css": ""
        }
    })
    .to_string();
    connection
        .execute("INSERT INTO col(ver, models) VALUES (11, ?1)", [&models])
        .expect("collection row");
    for id in 1..=2_i64 {
        let fields = format!("question {id}\u{1f}answer {id}");
        connection
            .execute(
                "INSERT INTO notes(id, mid, flds) VALUES (?1, 1, ?2)",
                params![id, fields.as_bytes()],
            )
            .expect("note row");
        connection
            .execute(
                "INSERT INTO cards(id, nid, ord, type, queue, due, ivl, factor, reps, lapses) VALUES (?1, ?1, 0, 0, 0, 0, 0, 0, 0, 0)",
                [id],
            )
            .expect("card row");
    }
    drop(connection);

    let mut database_bytes = Vec::new();
    File::open(&database)
        .expect("open fixture bytes")
        .read_to_end(&mut database_bytes)
        .expect("read fixture bytes");
    let output_file = File::create(&output).expect("create fixture archive");
    let mut archive = ZipWriter::new(output_file);
    archive
        .start_file("collection.anki21", SimpleFileOptions::default())
        .expect("start database entry");
    archive
        .write_all(&database_bytes)
        .expect("write database entry");
    archive.finish().expect("finish fixture archive");
    fs::remove_file(database).expect("remove fixture database");
}
