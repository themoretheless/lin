use lin::{Cell,Db,Row};
use rustc_hash::FxHashMap;
use std::{hint::black_box,time::Instant};
fn row_text<'a>(row:&'a Row,key:&str)->Option<&'a str>{row.get(key).and_then(Cell::text)}
fn add_posting(postings: &mut FxHashMap<String, Vec<usize>>, tok: &str, row_idx: usize) {
    if let Some(list) = postings.get_mut(tok) {
        if let Some(&last) = list.last() {
            if last == row_idx {
                return;
            }
            if last < row_idx {
                list.push(row_idx);
                return;
            }
        }
        if let Err(pos) = list.binary_search(&row_idx) {
            list.insert(pos, row_idx);
        }
    } else {
        postings.insert(tok.to_owned(), vec![row_idx]);
    }
}

fn fts_texts<'a: 'b, 'b>(row: &'a Row, fields: &'b [String]) -> impl Iterator<Item = &'a str> + 'b {
    let snippet = if fields.iter().any(|f| f == "snippet") {
        None
    } else {
        row_text(row, "snippet")
    };
    fields
        .iter()
        .filter_map(move |f| row_text(row, f))
        .chain(snippet)
}

fn lowercased<'a>(text: &'a str, scratch: &'a mut String) -> &'a str {
    if text.is_ascii() {
        // Fast path: check if already lowercase using byte-level SIMD detection
        // The .any() iterator is auto-vectorized by rustc for contiguous memory
        if !text.bytes().any(|b| b.is_ascii_uppercase()) {
            return text;
        }

        scratch.clear();
        // make_ascii_lowercase() uses single-instruction conversion on modern CPUs:
        // x86_64: AND with 0xDF to preserve case + OR with 0x20 unconditionally
        // ARM NEON: vbic/vorr pattern for selective bit manipulation
        scratch.reserve(text.len());
        scratch.push_str(text);
        scratch.make_ascii_lowercase();
        return scratch;
    }

    if !text.chars().any(|c| c.to_lowercase().next() != Some(c)) {
        return text;
    }
    scratch.clear();
    for c in text.chars() {
        scratch.extend(c.to_lowercase());
    }
    scratch
}
fn fixture(n:usize) -> Vec<Row> {
    let source=format!("insert docs [{}]",(0..n).map(|i|{
        let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
        let wing=if i%2==0 {"rag"}else{"sys"};
        format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000}else{30*86400000})
    }).collect::<Vec<_>>().join(","));
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source).unwrap().done.n,n);
    db.store.collection("docs").to_vec()
}

fn build(rows:&[Row],reserve:usize)->FxHashMap<String,Vec<usize>> {
 let mut postings=FxHashMap::default();if reserve==1 {postings.reserve(rows.len().min(4096));}
 let fields=vec!["title".to_owned(),"body".to_owned()];let mut scratch=String::with_capacity(256);
 for (i,row) in rows.iter().enumerate(){
 if reserve==2 && i==32 && rows.len()>=128 && postings.len()>=24 {
  let target=rows.len().min(4096).min(postings.len().saturating_mul(8));
  postings.reserve(target.saturating_sub(postings.len()));
 }
 for text in fts_texts(row,&fields){let lowered=lowercased(text,&mut scratch);for tok in lowered.split_whitespace(){add_posting(&mut postings,tok,i);}}}
 postings
}
fn main(){
 let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 let mut repeated=fixture(1000);for row in &mut repeated{row.insert("title".into(),Cell::from("common common"));row.insert("body".into(),Cell::from("common repeated"));}
 let mut mixed=fixture(1000);for row in &mut mixed{row.insert("title".into(),Cell::from("HELLO Σß 😀\u{000b}hello"));row.insert("snippet".into(),Cell::from("Hello\u{2003}new"));}
 let mut skew=fixture(1000);for row in skew.iter_mut().skip(32){row.insert("title".into(),Cell::from("common"));row.insert("body".into(),Cell::from("common"));}
 for (name,rows,loops) in [("default_1k",fixture(1000),16),("default_10k",fixture(10000),2),("repeated_1k",repeated,16),("unicode_1k",mixed,16),("prefix_skew_1k",skew,16)] {
 let expected=build(&rows,0);let reserves=[0,1,2];let mut times=vec![Vec::<f64>::new();3];let mut caps=Vec::new();
 for cap in reserves {let got=build(&rows,cap);assert_eq!(got,expected);caps.push(got.capacity());}
 for round in 0..24 {for pos in 0..3 {let v=(pos+round+process)%3;let t=Instant::now();for _ in 0..loops{black_box(build(black_box(&rows),reserves[v]));}times[v].push(t.elapsed().as_nanos() as f64/loops as f64);}}
 println!("{}",serde_json::json!({"case":name,"ns":times,"capacities":caps,"terms":expected.len(),"reserves":reserves,"exact_postings":true}));
 }
}
