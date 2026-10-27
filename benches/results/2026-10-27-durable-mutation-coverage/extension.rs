// Full-sync mutation coverage. This adds a separate contract; native cases stay unchanged.
#[derive(Clone, Copy)]
enum DurableOp { Update, Delete, Commit, Rollback }
impl DurableOp {
    fn name(self) -> &'static str { match self { Self::Update => "update", Self::Delete => "delete", Self::Commit => "commit", Self::Rollback => "rollback" } }
}
struct DurableLinWrite {
    write: std::cell::RefCell<Option<LinWrite>>,
    expected: Vec<serde_json::Value>,
    dir: TmpKeep,
}
fn durable_json_row(row: &lin::Row) -> serde_json::Value {
    serde_json::Value::Object(row.iter().map(|(k,v)| {
        let value = match v.text() {
            Some(text) => serde_json::Value::String(text.to_owned()),
            None => serde_json::to_value(v).unwrap(),
        };
        (k.clone(),value)
    }).collect())
}
fn durable_expected_lin(db: &lin::Db, op: DurableOp, id: &str) -> Vec<serde_json::Value> {
    // Serialize now so the oracle retains no shared Row/Cell owners during timing.
    let mut expected: Vec<serde_json::Value> = db.store.collection("docs").iter()
        .map(durable_json_row).collect();
    let pos = expected.iter().position(|v| v["id"].as_str() == Some(id)).unwrap();
    match op {
        DurableOp::Update | DurableOp::Commit => { expected[pos]["wing"] = serde_json::Value::String("sys".into()); }
        DurableOp::Delete => { expected.remove(pos); }
        DurableOp::Rollback => {},
    }
    expected.sort_by_key(|v| v["id"].to_string());
    expected
}
fn setup_durable_lin_write(n: usize, op: DurableOp) -> DurableLinWrite {
    let dir = fresh_tmp("durable-mutation-lin");
    let db = lin::Db::open_with(&dir.0, lin::OpenOpts { sync: lin::SyncMode::Full, cold: false }).unwrap();
    let write = setup_lin_write_db(n, db);
    let expected = durable_expected_lin(&write.db, op, &write.probe_id);
    DurableLinWrite { write: std::cell::RefCell::new(Some(write)), expected, dir }
}
fn run_durable_lin_write(s: &mut DurableLinWrite, op: DurableOp) -> usize {
    let w = s.write.get_mut().as_mut().unwrap();
    match op { DurableOp::Update => write_update_lin(w), DurableOp::Delete => write_delete_lin(w), DurableOp::Commit => tx_update_commit_lin(w), DurableOp::Rollback => tx_update_rollback_lin(w) }
}
fn verify_durable_lin_write(s: &DurableLinWrite, affected: &usize) -> airbug_bench::Result<()> {
    assert_eq!(*affected, 1);
    drop(s.write.borrow_mut().take().unwrap());
    let db = lin::Db::open_with(&s.dir.0, lin::OpenOpts { sync: lin::SyncMode::Full, cold: false }).unwrap();
    let mut actual: Vec<serde_json::Value> = db.store.collection("docs").iter().map(durable_json_row).collect();
    actual.sort_by_key(|v| v["id"].to_string());
    assert_eq!(actual, s.expected, "full Lin values after reopen");
    assert_eq!(db.store.row_count(), s.expected.len());
    Ok(())
}
struct DurableSqliteWrite {
    write: std::cell::RefCell<Option<SqlWarm>>,
    expected: Vec<(String,String,String,String,i64,String)>,
    dir: TmpKeep,
}
fn setup_durable_sqlite_write(n: usize, op: DurableOp, journal: &str) -> DurableSqliteWrite {
    let base = setup_sqlite_durable("docs", n);
    assert!(matches!(journal, "delete" | "wal"));
    base.conn.execute_batch(&format!("PRAGMA journal_mode = {journal};")).unwrap();
    let rows = docs(n);
    fill_sqlite_rows(&base.conn, &rows);
    assert_eq!(base.conn.query_row("PRAGMA synchronous", [], |r| r.get::<_,i64>(0)).unwrap(), 2);
    assert_eq!(base.conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_,String>(0)).unwrap(), journal);
    let probe_id = rows[PROBE].id.clone();
    base.conn.prepare_cached("UPDATE docs SET wing = ?1 WHERE id = ?2").unwrap();
    base.conn.prepare_cached("DELETE FROM docs WHERE id = ?1").unwrap();
    let mut expected: Vec<_> = rows.into_iter().filter_map(|mut r| {
        if r.id == probe_id { match op {
            DurableOp::Delete => return None,
            DurableOp::Update | DurableOp::Commit => r.wing = "sys".into(),
            DurableOp::Rollback => {},
        } }
        Some((r.id,r.uri,r.wing,r.title,r.ts,r.body))
    }).collect();
    expected.sort();
    DurableSqliteWrite { write: std::cell::RefCell::new(Some(SqlWarm { conn: base.conn, probe_id })), expected, dir: base._dir }
}
fn run_durable_sqlite_write(s: &mut DurableSqliteWrite, op: DurableOp) -> usize {
    let w = s.write.get_mut().as_mut().unwrap();
    match op { DurableOp::Update => write_update_sqlite(w), DurableOp::Delete => write_delete_sqlite(w), DurableOp::Commit => tx_update_commit_sqlite(w), DurableOp::Rollback => tx_update_rollback_sqlite(w) }
}
fn verify_durable_sqlite_write(s: &DurableSqliteWrite, affected: &usize) -> airbug_bench::Result<()> {
    assert_eq!(*affected, 1);
    let write = s.write.borrow_mut().take().unwrap();
    write.conn.close().expect("close SQLite before reopen");
    let conn = rusqlite::Connection::open(s.dir.0.join("db.sqlite")).unwrap();
    let mut stmt = conn.prepare("SELECT id,uri,wing,title,ts,body FROM docs ORDER BY id").unwrap();
    let actual: Vec<(String,String,String,String,i64,String)> = stmt.query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap().map(Result::unwrap).collect();
    assert_eq!(actual, s.expected, "full SQLite values after reopen");
    Ok(())
}
fn register_durable_mutations(suite: &mut Suite) {
    for (label, n) in [("1k", 1000usize), ("10k", 10000usize)] {
        for op in [DurableOp::Update,DurableOp::Delete,DurableOp::Commit,DurableOp::Rollback] {
            suite.bench_checked(&format!("durable_{}_1row_{label}/lin", op.name()),
                move || setup_durable_lin_write(n,op), move |s| run_durable_lin_write(s,op), verify_durable_lin_write, DropPolicy::OutsideTiming)
                .tag("durable").tag("write").tag("lin").parameter("rows",n).work_units("stmts",1);
            for journal in ["delete", "wal"] {
            suite.bench_checked(&format!("durable_{}_1row_{label}/sqlite_{journal}", op.name()),
                move || setup_durable_sqlite_write(n,op,journal), move |s| run_durable_sqlite_write(s,op), verify_durable_sqlite_write, DropPolicy::OutsideTiming)
                .tag("durable").tag("write").tag("sqlite").parameter("rows",n).work_units("stmts",1);
            }
        }
    }
}
