//! Error values are safe to show: rendering any error (Display and Debug) never panics,
//! a statement carrying a huge literal does not produce a huge message, odd input such as
//! control characters, lone surrogate escapes and NUL bytes renders as text, and the
//! position points inside the statement.

use lin::Db;

#[test]
fn errors_render_without_panic_and_stay_bounded() {
    let mut db = Db::empty();
    db.run("col t { id: text, v: i64 }").unwrap();
    let huge = "x".repeat(1_000_000);
    let cases: Vec<String> = vec![
        String::new(),
        "|".into(),
        "t | v ==".into(),
        "t | v == \"text\" | count".into(),
        "nosuch | take all".into(),
        format!("t | id == \"{huge}\" and v == \"oops\" | count"),
        format!("insert t {{ id: \"a\", v: \"{huge}\" }}"),
        format!("{huge}"),
        "t | id == \"\u{0}\u{1}\u{7f}\" and v == \"x\"".into(),
        "t | id == \"\\u{d800}\" | count".into(),
        "insert t { id: \"a\", v: 9223372036854775808 }".into(),
        "t | sort v | take -1".into(),
        "((((((((((((((((((((((((((((((((((((((((((((((((((".into(),
    ];
    for src in &cases {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| db.run(src)));
        let res = res.unwrap_or_else(|_| panic!("run panicked on {:?}", &src[..src.len().min(60)]));
        let Err(err) = res else { continue };
        let shown = std::panic::catch_unwind(|| (format!("{err}"), format!("{err:?}")));
        let (display, debug) = shown.unwrap_or_else(|_| panic!("rendering panicked for {:?}", &src[..src.len().min(60)]));
        assert!(!display.is_empty());
        assert!(display.len() < 20_000, "display of {} bytes for a {}-byte statement", display.len(), src.len());
        assert!(debug.len() < 20_000, "debug of {} bytes for a {}-byte statement", debug.len(), src.len());
        if let Some(pos) = err.pos {
            assert!(pos <= src.len(), "position {pos} past the end of a {}-byte statement", src.len());
        }
    }
}
