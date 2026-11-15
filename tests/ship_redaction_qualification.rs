//! Replication errors never carry the shared token: a wrong token, an unreachable peer
//! and a server that refuses the client all produce errors (on both sides) whose text
//! and debug form contain neither the client's nor the server's token, and the options
//! structs redact tokens in their debug form.

use std::net::TcpListener;

use lin::Db;
use lin::ship::{PullOpts, ServeOpts, pull_with, serve_one_opts};

const SERVER_TOKEN: &str = "server-token-7f3a9c";
const CLIENT_TOKEN: &str = "client-token-b81e44";

fn clean(what: &str, text: &str) {
    assert!(!text.contains(SERVER_TOKEN), "{what} leaks the server token: {text}");
    assert!(!text.contains(CLIENT_TOKEN), "{what} leaks the client token: {text}");
}

#[test]
fn tokens_never_appear_in_errors_or_debug_output() {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("lin-ship-redact-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Db::open(&dir).unwrap();
    db.run(r#"insert docs { uri: "raw://r", title: "R", layer: "raw" }"#).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let serve_opts = ServeOpts { token: Some(SERVER_TOKEN.into()), tls: None };
    clean("ServeOpts debug", &format!("{serve_opts:?}"));
    let server = std::thread::spawn(move || {
        let r = serve_one_opts(&db, &listener, &serve_opts);
        r.err().map(|e| (format!("{e}"), format!("{e:?}")))
    });
    let pull_opts = PullOpts { token: Some(CLIENT_TOKEN.into()), ..Default::default() };
    clean("PullOpts debug", &format!("{pull_opts:?}"));
    let err = pull_with(addr, 0, &pull_opts).expect_err("wrong token accepted");
    clean("client error", &format!("{err}"));
    clean("client error debug", &format!("{err:?}"));
    if let Some((shown, debug)) = server.join().unwrap() {
        clean("server error", &shown);
        clean("server error debug", &debug);
    }
    // Nobody listening.
    let closed = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let err = pull_with(closed, 0, &pull_opts).expect_err("pull from a closed port succeeded");
    clean("unreachable error", &format!("{err} {err:?}"));
}
