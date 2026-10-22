//! Rebuildable SQLite index of the model in `.clewtrace/index.db` under the project root.
//!
//! The `.sysml` text files stay the source of truth. The index is a cache: `rebuild`
//! recreates it from the text, and a missing, corrupt, or outdated database is rebuilt on
//! open. `update` re-parses only the files whose content changed since the last run.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{Connection, ErrorCode, Transaction, params};
use sha2::{Digest, Sha256};

use crate::parse::ast::{Kind, Member, UsageKind};
use crate::parse::parser;

// @implements CT-SEC-003
/// Folder under the project root that holds the index.
pub const INDEX_DIR: &str = ".clewtrace";

// @implements CT-SEC-003
/// File name of the index database inside `INDEX_DIR`.
pub const INDEX_FILE: &str = "index.db";

// @implements CT-MOD-001
/// Stored in `PRAGMA user_version`. A database with another version is rebuilt.
const SCHEMA_VERSION: i64 = 2;

// @implements CT-MOD-001, CT-EFF-007
const SCHEMA: &str = "
CREATE TABLE files (
    path TEXT PRIMARY KEY,
    size INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    sha256 BLOB NOT NULL
);
CREATE TABLE elements (
    id INTEGER PRIMARY KEY,
    file TEXT NOT NULL,
    kind TEXT NOT NULL,
    name TEXT,
    short_name TEXT,
    qualified_name TEXT NOT NULL,
    line INTEGER NOT NULL,
    col INTEGER NOT NULL,
    doc TEXT NOT NULL
);
CREATE INDEX elements_file ON elements (file);
CREATE TABLE problems (
    file TEXT NOT NULL,
    seq INTEGER NOT NULL,
    text TEXT NOT NULL,
    PRIMARY KEY (file, seq)
);
CREATE VIRTUAL TABLE elements_fts USING fts5 (
    name, short_name, qualified_name, doc,
    content = 'elements', content_rowid = 'id'
);
CREATE TRIGGER elements_insert AFTER INSERT ON elements BEGIN
    INSERT INTO elements_fts (rowid, name, short_name, qualified_name, doc)
    VALUES (new.id, new.name, new.short_name, new.qualified_name, new.doc);
END;
CREATE TRIGGER elements_delete AFTER DELETE ON elements BEGIN
    INSERT INTO elements_fts (elements_fts, rowid, name, short_name, qualified_name, doc)
    VALUES ('delete', old.id, old.name, old.short_name, old.qualified_name, old.doc);
END;
";

// @implements CT-USE-007
/// An index error: what went wrong, where, and how to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexError {
    pub path: String,
    pub message: String,
    pub help: &'static str,
}

// @implements CT-USE-007
impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: error: {}\n  help: {}",
            self.path, self.message, self.help
        )
    }
}

// @implements CT-EFF-007, CT-USE-007
/// What one `update` or `rebuild` did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UpdateStats {
    /// `.sysml` files found under the project root.
    pub files: usize,
    /// Files parsed again because they are new or their content changed.
    pub parsed: usize,
    /// Files dropped from the index because they no longer exist.
    pub removed: usize,
    /// Syntax errors in every indexed file, whether parsed on this run or not, and files
    /// that could not be read, each rendered as `path:line:column: error: ...` or
    /// `path: error: ...` with a `help:` line, sorted by path.
    pub problems: Vec<String>,
}

// @implements CT-EFF-006
/// One search result, pointing at the element's place in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub file: String,
    pub line: u32,
    pub column: u32,
    /// For example `requirement def` or `part`.
    pub kind: String,
    pub qualified_name: String,
    pub short_name: Option<String>,
}

// @implements CT-MOD-001
/// One indexed element, before it is written.
struct Row {
    kind: &'static str,
    name: Option<String>,
    short_name: Option<String>,
    qualified_name: String,
    line: u32,
    column: u32,
    doc: String,
}

// @implements CT-EFF-007
/// A file's content hash and text, read in one go.
struct Loaded {
    sha256: Vec<u8>,
    text: String,
}

// @implements CT-MOD-001, CT-EFF-007, CT-SEC-003
/// An open index for one project root.
pub struct Index {
    root: PathBuf,
    conn: Connection,
}

// @implements CT-MOD-001, CT-EFF-007, CT-SEC-003
impl Index {
    /// Opens `.clewtrace/index.db` under `root`, creating it if needed. A database that is
    /// corrupt or has an older schema is replaced by an empty one; `update` then fills it.
    pub fn open(root: &Path) -> Result<Index, IndexError> {
        let dir = root.join(INDEX_DIR);
        fs::create_dir_all(&dir).map_err(|e| IndexError {
            path: dir.display().to_string(),
            message: format!("cannot create the index folder: {e}"),
            help: "make sure the project folder is writable and .clewtrace is not a file",
        })?;
        let db = dir.join(INDEX_FILE);
        let conn = match open_schema(&db) {
            Err(e) if e.sqlite_error_code() == Some(ErrorCode::NotADatabase) => {
                // The index is only a cache, so a damaged file is replaced, never repaired.
                fs::remove_file(&db).map_err(|e| io_error(&db, e))?;
                open_schema(&db)
            }
            other => other,
        }
        .map_err(|e| sql_error(&db, e))?;
        Ok(Index {
            root: root.to_path_buf(),
            conn,
        })
    }

    /// Re-indexes the `.sysml` files under the root whose content changed, adds new files,
    /// and drops files that no longer exist. Unchanged files are not read again.
    pub fn update(&mut self) -> Result<UpdateStats, IndexError> {
        let found = sysml_files(&self.root)?;
        let db = self.db_path();
        let tx = self.conn.transaction().map_err(|e| sql_error(&db, e))?;
        let stats = update_files(&tx, &self.root, &found).map_err(|e| sql_error(&db, e))?;
        tx.commit().map_err(|e| sql_error(&db, e))?;
        Ok(stats)
    }

    /// Drops everything in the index and indexes every file again from the text.
    pub fn rebuild(&mut self) -> Result<UpdateStats, IndexError> {
        let db = self.db_path();
        self.conn
            .execute_batch("DELETE FROM elements; DELETE FROM problems; DELETE FROM files;")
            .map_err(|e| sql_error(&db, e))?;
        self.update()
    }

    /// Number of indexed elements.
    pub fn element_count(&self) -> Result<i64, IndexError> {
        self.conn
            .query_row("SELECT count(*) FROM elements", [], |r| r.get(0))
            .map_err(|e| sql_error(&self.db_path(), e))
    }

    // @implements CT-EFF-006
    /// Full-text search over element names, short names, qualified names, and doc text.
    /// Every word must match. Returns one page of hits, best match first.
    pub fn search(&self, text: &str, limit: u32, offset: u32) -> Result<Vec<Hit>, IndexError> {
        let query = fts_query(text);
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let db = self.db_path();
        let mut stmt = self
            .conn
            .prepare(
                "SELECT e.file, e.line, e.col, e.kind, e.qualified_name, e.short_name
                 FROM elements_fts JOIN elements e ON e.id = elements_fts.rowid
                 WHERE elements_fts MATCH ?1
                 ORDER BY bm25(elements_fts), e.file, e.line
                 LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| sql_error(&db, e))?;
        let rows = stmt
            .query_map(params![query, limit, offset], |r| {
                Ok(Hit {
                    file: r.get(0)?,
                    line: r.get(1)?,
                    column: r.get(2)?,
                    kind: r.get(3)?,
                    qualified_name: r.get(4)?,
                    short_name: r.get(5)?,
                })
            })
            .map_err(|e| sql_error(&db, e))?;
        rows.collect::<Result<_, _>>()
            .map_err(|e| sql_error(&db, e))
    }

    fn db_path(&self) -> PathBuf {
        self.root.join(INDEX_DIR).join(INDEX_FILE)
    }
}

// @implements CT-MOD-001
/// Opens the database and makes sure it has the current schema, recreating it if not.
fn open_schema(db: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db)?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version != SCHEMA_VERSION {
        conn.execute_batch(
            "DROP TABLE IF EXISTS elements_fts; DROP TABLE IF EXISTS elements;
             DROP TABLE IF EXISTS problems; DROP TABLE IF EXISTS files;",
        )?;
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(conn)
}

// @implements CT-EFF-007, CT-USE-007
/// Brings the index in line with `found`, the relative paths of every `.sysml` file.
fn update_files(
    tx: &Transaction<'_>,
    root: &Path,
    found: &[String],
) -> rusqlite::Result<UpdateStats> {
    let mut known: HashMap<String, (i64, i64, Vec<u8>)> = HashMap::new();
    {
        let mut stmt = tx.prepare("SELECT path, size, mtime_ns, sha256 FROM files")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?;
        for row in rows {
            let (path, entry) = row?;
            known.insert(path, entry);
        }
    }
    let mut stats = UpdateStats {
        files: found.len(),
        ..UpdateStats::default()
    };
    // Problems by path, so stored and fresh ones come out in one sorted list.
    let mut problems: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for rel in found {
        let abs = root.join(rel);
        let (size, mtime) = fs::metadata(&abs).map_or((-1, -1), |m| stamp(&m));
        let previous = known.remove(rel);
        if let Some((s, m, _)) = &previous
            && (*s, *m) == (size, mtime)
        {
            continue;
        }
        let loaded = match read_file(&abs) {
            Ok(loaded) => loaded,
            Err(message) => {
                problems.entry(rel.clone()).or_default().push(format!(
                    "{rel}: error: {message}\n  help: save the file as UTF-8 text and make sure it is readable"
                ));
                forget(tx, rel)?;
                continue;
            }
        };
        if previous.is_some_and(|(_, _, hash)| hash == loaded.sha256) {
            // Touched but not changed: remember the new stamp and skip the parse.
            tx.execute(
                "UPDATE files SET size = ?2, mtime_ns = ?3 WHERE path = ?1",
                params![rel, size, mtime],
            )?;
            continue;
        }
        let parsed = parser::parse(&loaded.text);
        forget(tx, rel)?;
        tx.execute(
            "INSERT INTO files (path, size, mtime_ns, sha256) VALUES (?1, ?2, ?3, ?4)",
            params![rel, size, mtime, loaded.sha256],
        )?;
        let mut insert =
            tx.prepare("INSERT INTO problems (file, seq, text) VALUES (?1, ?2, ?3)")?;
        for (seq, error) in (0_i64..).zip(&parsed.errors) {
            insert.execute(params![rel, seq, error.render(rel)])?;
        }
        let mut rows = Vec::new();
        collect_rows(&parsed.members, "", &mut rows);
        let mut insert = tx.prepare(
            "INSERT INTO elements (file, kind, name, short_name, qualified_name, line, col, doc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for row in rows {
            insert.execute(params![
                rel,
                row.kind,
                row.name,
                row.short_name,
                row.qualified_name,
                row.line,
                row.column,
                row.doc
            ])?;
        }
        stats.parsed += 1;
    }
    for rel in known.keys() {
        forget(tx, rel)?;
        stats.removed += 1;
    }
    // Every indexed file's syntax errors, so an unchanged broken file is still reported.
    let mut stmt = tx.prepare("SELECT file, text FROM problems ORDER BY file, seq")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?;
    for row in rows {
        let (rel, text) = row?;
        problems.entry(rel).or_default().push(text);
    }
    stats.problems = problems.into_values().flatten().collect();
    Ok(stats)
}

// @implements CT-EFF-007
/// Removes a file, its elements, and its problems from the index.
fn forget(tx: &Transaction<'_>, rel: &str) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM elements WHERE file = ?1", [rel])?;
    tx.execute("DELETE FROM problems WHERE file = ?1", [rel])?;
    tx.execute("DELETE FROM files WHERE path = ?1", [rel])?;
    Ok(())
}

// @implements CT-EFF-007
/// A file's size and modification time in nanoseconds, the cheap test for a change.
fn stamp(meta: &fs::Metadata) -> (i64, i64) {
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(-1, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX));
    (i64::try_from(meta.len()).unwrap_or(i64::MAX), mtime)
}

// @implements CT-EFF-007, CT-USE-007
/// Reads a file, hashes it with SHA-256, and decodes it as UTF-8.
fn read_file(abs: &Path) -> Result<Loaded, String> {
    let bytes = fs::read(abs).map_err(|e| format!("cannot read the file: {e}"))?;
    let sha256 = Sha256::digest(&bytes).to_vec();
    let text = String::from_utf8(bytes).map_err(|_| "the file is not UTF-8 text".to_string())?;
    Ok(Loaded { sha256, text })
}

// @implements CT-MOD-001
/// Flattens the named members of one file, depth first, into index rows.
fn collect_rows(members: &[Member], prefix: &str, rows: &mut Vec<Row>) {
    for member in members {
        let (kind, name, short_name) = match member {
            Member::Package(p) => ("package", p.name.clone(), p.short_name.clone()),
            Member::Definition(d) => (
                definition_kind(d.kind),
                d.name.clone(),
                d.short_name.clone(),
            ),
            Member::Usage(u) => (
                usage_kind(u.kind),
                u.effective_name().map(str::to_string),
                u.short_name.clone(),
            ),
            Member::Dependency(d) => ("dependency", d.name.clone(), d.short_name.clone()),
            Member::Satisfy(_) | Member::Verify(_) => {
                collect_rows(member.members(), prefix, rows);
                continue;
            }
            Member::Import(_) | Member::Comment(_) => continue,
        };
        let qualified_name = match name.as_ref().or(short_name.as_ref()) {
            Some(segment) if prefix.is_empty() => segment.clone(),
            Some(segment) => format!("{prefix}::{segment}"),
            None => prefix.to_string(),
        };
        let doc = member
            .members()
            .iter()
            .filter_map(|m| match m {
                Member::Comment(c) if c.is_doc => Some(c.body.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let pos = member.pos();
        collect_rows(member.members(), &qualified_name, rows);
        rows.push(Row {
            kind,
            name,
            short_name,
            qualified_name,
            line: pos.line,
            column: pos.column,
            doc,
        });
    }
}

// @implements CT-MOD-001
fn definition_kind(kind: Kind) -> &'static str {
    match kind {
        Kind::Part => "part def",
        Kind::Attribute => "attribute def",
        Kind::Port => "port def",
        Kind::Requirement => "requirement def",
        Kind::Metadata => "metadata def",
    }
}

// @implements CT-MOD-001
fn usage_kind(kind: UsageKind) -> &'static str {
    match kind {
        UsageKind::Of(Kind::Part) => "part",
        UsageKind::Of(Kind::Attribute) => "attribute",
        UsageKind::Of(Kind::Port) => "port",
        UsageKind::Of(Kind::Requirement) => "requirement",
        UsageKind::Of(Kind::Metadata) => "metadata",
        UsageKind::Subject => "subject",
    }
}

// @implements CT-EFF-006
/// Turns search text into an FTS5 query that cannot be a syntax error: each word becomes a
/// quoted phrase, so `CT-EFF-007` matches the short name and `"` is taken literally.
fn fts_query(text: &str) -> String {
    text.split_whitespace()
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

// @implements CT-MOD-001, CT-SEC-003
/// Relative paths, with `/` separators and sorted, of the `.sysml` files under `root`.
/// Skips folders whose name starts with `.` (such as `.git` and `.clewtrace`), folders
/// marked as caches by a `CACHEDIR.TAG` file (such as Cargo's `target`), and symbolic
/// links, so the index never reads outside the project folder.
pub fn sysml_files(root: &Path) -> Result<Vec<String>, IndexError> {
    let mut found = Vec::new();
    let mut pending = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, rel)) = pending.pop() {
        if dir.join("CACHEDIR.TAG").is_file() {
            continue;
        }
        let entries = fs::read_dir(&dir).map_err(|e| io_error(&dir, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| io_error(&dir, e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let file_type = entry.file_type().map_err(|e| io_error(&entry.path(), e))?;
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if file_type.is_dir() && !name.starts_with('.') {
                pending.push((entry.path(), child));
            } else if file_type.is_file() && name.ends_with(".sysml") {
                found.push(child);
            }
        }
    }
    found.sort();
    Ok(found)
}

// @implements CT-USE-007
fn io_error(path: &Path, e: std::io::Error) -> IndexError {
    IndexError {
        path: path.display().to_string(),
        message: e.to_string(),
        help: "make sure the project folder exists and is readable",
    }
}

// @implements CT-USE-007
fn sql_error(db: &Path, e: rusqlite::Error) -> IndexError {
    IndexError {
        path: db.display().to_string(),
        message: format!("index database error: {e}"),
        help: "run `clew index --rebuild`; if that fails, delete the .clewtrace folder, which is only a cache",
    }
}

// @verifies CT-MOD-001, CT-EFF-007, CT-EFF-006, CT-SEC-003, CT-USE-007
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    // @verifies CT-MOD-001
    fn project(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join("clew-index-tests").join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("model")).unwrap();
        fs::write(
            root.join("model/reqs.sysml"),
            "package Reqs {\n    requirement def <'R'> Req;\n    requirement <'CT-EFF-007'> incrementalIndex : Req {\n        doc /* Re-index only the files that changed. */\n    }\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("model/parts.sysml"),
            "package Parts {\n    part def Vehicle {\n        part engine : Engine;\n    }\n    part def Engine;\n}\n",
        )
        .unwrap();
        root
    }

    // @verifies CT-EFF-007
    fn set_mtime(path: &Path, secs: u64) {
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    }

    // @verifies CT-MOD-001, CT-SEC-003
    #[test]
    fn indexes_every_sysml_file_into_the_project_folder() {
        let root = project("first-run");
        let mut index = Index::open(&root).unwrap();
        let stats = index.update().unwrap();
        assert_eq!(
            stats,
            UpdateStats {
                files: 2,
                parsed: 2,
                removed: 0,
                problems: vec![]
            }
        );
        assert!(root.join(".clewtrace/index.db").is_file());
        assert_eq!(index.element_count().unwrap(), 7);
    }

    // @verifies CT-EFF-006, CT-MOD-001
    #[test]
    fn finds_elements_by_short_name_name_and_doc_text() {
        let root = project("search");
        let mut index = Index::open(&root).unwrap();
        index.update().unwrap();
        let hits = index.search("CT-EFF-007", 10, 0).unwrap();
        assert_eq!(
            hits,
            vec![Hit {
                file: "model/reqs.sysml".to_string(),
                line: 3,
                column: 5,
                kind: "requirement".to_string(),
                qualified_name: "Reqs::incrementalIndex".to_string(),
                short_name: Some("CT-EFF-007".to_string()),
            }]
        );
        let hits = index.search("changed files", 10, 0).unwrap();
        assert_eq!(hits[0].qualified_name, "Reqs::incrementalIndex");
        let hits = index.search("vehicle engine", 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].qualified_name, "Parts::Vehicle::engine");
        assert_eq!(hits[0].kind, "part");
        assert!(index.search("  ", 10, 0).unwrap().is_empty());
        assert!(index.search("\"quoted", 10, 0).unwrap().is_empty());
    }

    // @verifies CT-MOD-001, CT-TRC-002
    #[test]
    fn indexes_named_dependencies() {
        let root = project("dependencies");
        fs::write(
            root.join("model/links.sysml"),
            "package Links {\n    #refinement dependency <'L1'> engineRefines from Parts::Engine to Reqs::incrementalIndex;\n    #evidence dependency Parts::Vehicle to Reqs::incrementalIndex;\n}\n",
        )
        .unwrap();
        let mut index = Index::open(&root).unwrap();
        assert!(index.update().unwrap().problems.is_empty());
        let hits = index.search("L1", 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "dependency");
        assert_eq!(hits[0].qualified_name, "Links::engineRefines");
    }

    // @verifies CT-MOD-001, CT-TRC-002
    #[test]
    fn indexes_metadata_definitions() {
        let root = project("metadata");
        fs::write(
            root.join("model/meta.sysml"),
            "package Meta {\n    metadata def <evidence> Evidence;\n}\n",
        )
        .unwrap();
        let mut index = Index::open(&root).unwrap();
        assert!(index.update().unwrap().problems.is_empty());
        let hits = index.search("evidence", 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "metadata def");
        assert_eq!(hits[0].qualified_name, "Meta::Evidence");
        assert_eq!(usage_kind(UsageKind::Of(Kind::Metadata)), "metadata");
    }

    // @verifies CT-EFF-006
    #[test]
    fn returns_hits_one_page_at_a_time() {
        let root = project("paging");
        let mut index = Index::open(&root).unwrap();
        index.update().unwrap();
        // "Parts" is in the qualified name of all four elements of parts.sysml.
        let all = index.search("Parts", 10, 0).unwrap();
        assert_eq!(all.len(), 4);
        let page = index.search("Parts", 2, 1).unwrap();
        assert_eq!(page, all[1..3].to_vec());
    }

    // @verifies CT-EFF-007
    #[test]
    fn re_indexes_only_changed_files() {
        let root = project("incremental");
        let mut index = Index::open(&root).unwrap();
        index.update().unwrap();
        let unchanged = index.update().unwrap();
        assert_eq!((unchanged.files, unchanged.parsed), (2, 0));

        fs::write(
            root.join("model/parts.sysml"),
            "package Parts {\n    part def Wheel;\n}\n",
        )
        .unwrap();
        let stats = index.update().unwrap();
        assert_eq!((stats.files, stats.parsed, stats.removed), (2, 1, 0));
        assert!(index.search("Vehicle", 10, 0).unwrap().is_empty());
        assert_eq!(index.search("Wheel", 10, 0).unwrap().len(), 1);
        assert_eq!(index.element_count().unwrap(), 5);
    }

    // @verifies CT-EFF-007
    #[test]
    fn skips_the_parse_when_a_touched_file_has_the_same_content() {
        let root = project("touched");
        let mut index = Index::open(&root).unwrap();
        index.update().unwrap();
        set_mtime(&root.join("model/parts.sysml"), 1_000_000);
        assert_eq!(index.update().unwrap().parsed, 0);
        // The new stamp was stored, so the next run does not even read the file.
        let stored: i64 = index
            .conn
            .query_row(
                "SELECT mtime_ns FROM files WHERE path = 'model/parts.sysml'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, 1_000_000_000_000_000);
    }

    // @verifies CT-EFF-007
    #[test]
    fn drops_deleted_files_and_adds_new_ones() {
        let root = project("added-removed");
        let mut index = Index::open(&root).unwrap();
        index.update().unwrap();
        fs::remove_file(root.join("model/parts.sysml")).unwrap();
        fs::write(root.join("new.sysml"), "part def Added;\n").unwrap();
        let stats = index.update().unwrap();
        assert_eq!((stats.files, stats.parsed, stats.removed), (2, 1, 1));
        assert!(index.search("Engine", 10, 0).unwrap().is_empty());
        assert_eq!(index.search("Added", 10, 0).unwrap()[0].file, "new.sysml");
    }

    // @verifies CT-MOD-001
    #[test]
    fn rebuild_re_indexes_everything_from_the_text() {
        let root = project("rebuild");
        let mut index = Index::open(&root).unwrap();
        index.update().unwrap();
        let stats = index.rebuild().unwrap();
        assert_eq!((stats.files, stats.parsed), (2, 2));
        assert_eq!(index.element_count().unwrap(), 7);
    }

    // @verifies CT-MOD-001
    #[test]
    fn skips_hidden_folders_cache_folders_and_other_files() {
        let root = project("walk");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/x.sysml"), "part def Hidden;\n").unwrap();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::write(
            root.join("target/CACHEDIR.TAG"),
            "Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        fs::write(root.join("target/debug/x.sysml"), "part def Built;\n").unwrap();
        fs::write(root.join("model/notes.txt"), "not a model").unwrap();
        assert_eq!(
            sysml_files(&root).unwrap(),
            vec!["model/parts.sysml", "model/reqs.sysml"]
        );
    }

    // @verifies CT-USE-007
    #[test]
    fn reports_syntax_errors_and_unreadable_files_with_their_path() {
        let root = project("problems");
        fs::write(
            root.join("model/broken.sysml"),
            "part def 42;\npart def Ok;\n",
        )
        .unwrap();
        fs::write(root.join("model/latin1.sysml"), b"part def Caf\xe9;\n").unwrap();
        let mut index = Index::open(&root).unwrap();
        let stats = index.update().unwrap();
        assert_eq!((stats.files, stats.parsed), (4, 3));
        assert_eq!(stats.problems.len(), 2, "{:?}", stats.problems);
        assert!(
            stats.problems[0].starts_with("model/broken.sysml:1:"),
            "{:?}",
            stats.problems
        );
        assert!(stats.problems[0].contains("help:"));
        assert_eq!(
            stats.problems[1],
            "model/latin1.sysml: error: the file is not UTF-8 text\n  help: save the file as UTF-8 text and make sure it is readable"
        );
        // What parsed is still indexed, so a typo does not hide the rest of the file.
        assert_eq!(index.search("Ok", 10, 0).unwrap().len(), 1);
    }

    // @verifies CT-USE-007, CT-EFF-007
    #[test]
    fn reports_the_same_problems_when_a_broken_file_is_unchanged() {
        let root = project("problems-kept");
        fs::write(
            root.join("model/broken.sysml"),
            "part def 42;\npart def Ok;\n",
        )
        .unwrap();
        fs::write(root.join("model/latin1.sysml"), b"part def Caf\xe9;\n").unwrap();
        let mut index = Index::open(&root).unwrap();
        let first = index.update().unwrap();
        let second = index.update().unwrap();
        assert_eq!(second.parsed, 0);
        assert_eq!(second.problems, first.problems);
        assert_eq!(second.problems.len(), 2);

        // A touched file with the same content keeps its problems too.
        set_mtime(&root.join("model/broken.sysml"), 1_000_000);
        assert_eq!(index.update().unwrap().problems, first.problems);

        // Fixing the file clears them, and a rebuild reports what is left.
        fs::write(root.join("model/broken.sysml"), "part def Ok;\n").unwrap();
        assert_eq!(index.update().unwrap().problems.len(), 1);
        assert_eq!(index.rebuild().unwrap().problems.len(), 1);
        fs::remove_file(root.join("model/latin1.sysml")).unwrap();
        assert!(index.update().unwrap().problems.is_empty());
    }

    // @verifies CT-EFF-007
    #[test]
    fn forgets_a_file_that_becomes_unreadable() {
        let root = project("unreadable");
        let mut index = Index::open(&root).unwrap();
        index.update().unwrap();
        fs::write(root.join("model/parts.sysml"), b"part def Caf\xe9;\n").unwrap();
        let stats = index.update().unwrap();
        assert_eq!(stats.problems.len(), 1);
        assert!(index.search("Vehicle", 10, 0).unwrap().is_empty());
    }

    // @verifies CT-MOD-001
    #[test]
    fn replaces_a_corrupt_or_outdated_database() {
        let root = project("corrupt");
        fs::create_dir_all(root.join(".clewtrace")).unwrap();
        fs::write(root.join(".clewtrace/index.db"), vec![b'x'; 4096]).unwrap();
        let mut index = Index::open(&root).unwrap();
        assert_eq!(index.update().unwrap().parsed, 2);
        index
            .conn
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        drop(index);
        let index = Index::open(&root).unwrap();
        assert_eq!(index.element_count().unwrap(), 0);
    }

    // @verifies CT-USE-007
    #[test]
    fn explains_errors_with_where_and_how_to_fix() {
        let root = project("errors");
        fs::write(root.join(".clewtrace"), "a file, not a folder").unwrap();
        let err = Index::open(&root).err().unwrap();
        assert!(
            err.message.starts_with("cannot create the index folder"),
            "{err}"
        );
        assert!(err.to_string().contains("\n  help: "), "{err}");

        let err = sysml_files(&root.join("missing")).unwrap_err();
        assert!(err.path.ends_with("missing"), "{err}");
        assert_eq!(
            err.help,
            "make sure the project folder exists and is readable"
        );

        fs::remove_file(root.join(".clewtrace")).unwrap();
        let index = Index::open(&root).unwrap();
        index.conn.execute_batch("DROP TABLE elements_fts").unwrap();
        let err = index.search("Vehicle", 10, 0).unwrap_err();
        assert!(err.message.starts_with("index database error:"), "{err}");
        assert!(err.help.contains("clew index --rebuild"));
    }
}
