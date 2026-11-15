//! The built-in embedder: empty text gets no vector (and is not a vector-search hit),
//! every other text gets a unit-length vector of one fixed dimension, case does not
//! change the vector, Unicode text is embedded as well as ASCII, repeating a word keeps
//! the direction close, and the same text gives the same bits in every database.

use lin::{Cell, Db};

fn embeddings(texts: &[&str]) -> Vec<Option<Vec<f32>>> {
    let mut db = Db::empty();
    for (i, t) in texts.iter().enumerate() {
        db.run(&format!("insert docs {{ uri: \"raw://e/{i}\", title: {t:?}, layer: \"raw\" }}")).unwrap();
    }
    (0..texts.len()).map(|i| {
        match db.run(&format!(r#"docs | uri == "raw://e/{i}" | {{ embedding }}"#)).unwrap().rows[0].get("embedding") {
            Some(Cell::Vec(v)) => Some(v.to_vec()),
            Some(Cell::Null) | None => None,
            other => panic!("unexpected embedding cell {other:?}"),
        }
    }).collect()
}

fn cos(a: &[f32], b: &[f32]) -> f64 {
    a.iter().zip(b).map(|(x, y)| f64::from(*x) * f64::from(*y)).sum()
}

#[test]
fn default_embedder_properties() {
    let texts = ["", "word", "WORD", "word word word", "кириллица текст", "漢字 テキスト", "a much longer sentence about birds and rivers and stones", "x"];
    let e = embeddings(&texts);
    assert!(e[0].is_none(), "empty text got a vector");
    let dim = e[1].as_ref().unwrap().len();
    for (t, v) in texts.iter().zip(&e).skip(1) {
        let v = v.as_ref().unwrap_or_else(|| panic!("{t:?}: no vector"));
        assert_eq!(v.len(), dim, "{t:?}: dimension");
        assert!(v.iter().all(|x| x.is_finite()), "{t:?}: non-finite coordinate");
        let norm = cos(v, v).sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "{t:?}: norm {norm}");
    }
    assert_eq!(e[1], e[2], "case changed the vector");
    assert!(cos(e[1].as_ref().unwrap(), e[3].as_ref().unwrap()) > 0.9, "repetition changed the direction");
    // The same texts in a fresh database: identical bits.
    assert_eq!(embeddings(&texts), e, "embedding is not deterministic");

    let mut db = Db::empty();
    db.run(r#"insert docs [{ uri: "raw://z/1", title: "", layer: "raw" }, { uri: "raw://z/2", title: "word", layer: "raw" }]"#).unwrap();
    let hits: Vec<String> = db.run(r#"docs | search vec "word" | { uri } | take all"#).unwrap().rows.iter().map(|r| r.get("uri").and_then(|c| c.text()).unwrap().to_string()).collect();
    assert_eq!(hits, ["raw://z/2"]);
    assert!(db.run(r#"docs | search vec "" | { uri } | take all"#).unwrap().rows.is_empty());
}
