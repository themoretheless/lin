//! A damaged snapshot file never opens as different data: for single-byte changes spread
//! across the whole file, and for truncations, opening either fails or yields exactly the
//! committed rows. The snapshot is the only copy here (the log is empty after the
//! checkpoint), so silently accepting a bad byte would be data corruption.

use lin::Db;

fn tmp() -> std::path::PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("lin-snap-corrupt-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn state(db: &mut Db) -> Vec<String> {
    let mut v: Vec<String> = db.run("t | { id, v, s } | take all").unwrap().rows.iter().map(|r| format!("{r:?}")).collect();
    v.sort();
    v
}

#[test]
fn damaged_snapshot_is_refused_or_exact() {
    let dir = tmp();
    let want;
    {
        let mut db = Db::open(&dir).unwrap();
        db.run("col t { id: text, v: i64, s: text }").unwrap();
        let ins = (0..20).map(|i| format!(r#"{{ id: "r{i}", v: {}, s: "text {i}" }}"#, i * 1000 + 7)).collect::<Vec<_>>().join(",");
        db.run(&format!("insert t [{ins}]")).unwrap();
        db.checkpoint().unwrap();
        want = state(&mut db);
        db.close().unwrap();
    }
    let snap_name = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().into_string().unwrap())
        .find(|n| n.starts_with("snapshot") && !n.ends_with(".tmp")).expect("snapshot file");
    let good = std::fs::read(dir.join(&snap_name)).unwrap();
    assert_eq!(std::fs::metadata(dir.join(lin::persist::LOG_NAME)).map(|m| m.len()).unwrap_or(0), 0);
    let mut accepted_wrong = Vec::new();
    let mut try_bytes = |what: String, bytes: &[u8]| {
        let image = tmp();
        for e in std::fs::read_dir(&dir).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_file() && e.file_name() != "LOCK" {
                std::fs::copy(e.path(), image.join(e.file_name())).unwrap();
            }
        }
        std::fs::write(image.join(&snap_name), bytes).unwrap();
        let res = std::panic::catch_unwind(|| Db::open(&image).map(|mut db| state(&mut db)));
        match res {
            Err(_) => accepted_wrong.push(format!("{what}: panicked")),
            Ok(Ok(s)) if s != want => accepted_wrong.push(what),
            _ => {}
        }
        let _ = std::fs::remove_dir_all(&image);
    };
    let step = (good.len() / 300).max(1);
    let mut i = 0;
    while i < good.len() {
        let mut b = good.clone();
        b[i] ^= 0x5a;
        try_bytes(format!("byte {i}/{} flipped", good.len()), &b);
        i += step;
    }
    for cut in [0, 1, good.len() / 3, good.len() / 2, good.len() - 4, good.len() - 1] {
        try_bytes(format!("truncated to {cut}"), &good[..cut]);
    }
    assert!(accepted_wrong.is_empty(), "opened with wrong data: {accepted_wrong:?}");

    // A snapshot in the earlier format (`0x03`, no checksum trailer) still opens.
    assert_eq!(good[0], 0x03);
    let legacy = good[..good.len() - 4].to_vec();
    let image = tmp();
    for e in std::fs::read_dir(&dir).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_file() && e.file_name() != "LOCK" {
            std::fs::copy(e.path(), image.join(e.file_name())).unwrap();
        }
    }
    std::fs::write(image.join(&snap_name), &legacy).unwrap();
    let mut db = Db::open(&image).unwrap();
    assert_eq!(state(&mut db), want, "legacy snapshot");
}
