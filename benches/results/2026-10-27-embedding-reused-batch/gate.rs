use lin::{Cell,Row,Embedder,HashingEmbedder};
use std::{sync::Arc,time::Instant,hint::black_box};
mod store {pub use lin::Row;pub fn row_text<'a>(r:&'a Row,k:&str)->Option<&'a str>{r.get(k).and_then(lin::Cell::text)}}
pub(crate) fn row_embed_text(row: &crate::store::Row) -> std::borrow::Cow<'_, str> {
    let texts = ["title", "body", "snippet"].map(|k| crate::store::row_text(row, k));
    let bytes = texts.iter().flatten().map(|t| t.len()).sum::<usize>();
    if bytes == 0 {
        return std::borrow::Cow::Borrowed("");
    }
    // Leading empty fields contribute no separator. Any present field after
    // the first nonempty one does, including an empty trailing field.
    if let Some(first) = texts.iter().position(|t| t.is_some_and(|s| !s.is_empty()))
        && texts[first + 1..].iter().all(Option::is_none)
    {
        return std::borrow::Cow::Borrowed(texts[first].unwrap());
    }
    let mut blob = String::with_capacity(bytes + 2);
    for text in texts.into_iter().flatten() {
        if !blob.is_empty() {
            blob.push(' ');
        }
        blob.push_str(text);
    }
    std::borrow::Cow::Owned(blob)
}
fn stream_text<'a>(row: &'a crate::store::Row, scratch: &'a mut String) -> &'a str {
    let texts = ["title", "body", "snippet"].map(|k| crate::store::row_text(row, k));
    let bytes = texts.iter().flatten().map(|t| t.len()).sum::<usize>();
    if bytes == 0 {
        return "";
    }
    // Leading empty fields contribute no separator. Any present field after
    // the first nonempty one does, including an empty trailing field.
    if let Some(first) = texts.iter().position(|t| t.is_some_and(|s| !s.is_empty()))
        && texts[first + 1..].iter().all(Option::is_none)
    {
        return texts[first].unwrap();
    }
    scratch.clear();
    let blob = scratch;
    for text in texts.into_iter().flatten() {
        if !blob.is_empty() {
            blob.push(' ');
        }
        blob.push_str(text);
    }
    blob
}

fn baseline(emb:&HashingEmbedder,rows:&[Row])->Vec<Arc<[f32]>> {lin::diagnostic_embedding_baseline(emb,rows)}
fn candidate(emb:&HashingEmbedder,rows:&[Row])->Vec<Arc<[f32]>> {lin::diagnostic_embedding_reused_batch(emb,rows)}
fn exact(a:&[Arc<[f32]>],b:&[Arc<[f32]>])->bool{a.len()==b.len() && a.iter().zip(b).all(|(a,b)|a.len()==b.len() && a.iter().zip(b.iter()).all(|(a,b)|a.to_bits()==b.to_bits()))}
fn fixture(n:usize,single:bool)->Vec<Row>{(0..n).map(|i|{
 let title=if i%10==0 {format!("doc {i} wal note")}else{format!("doc {i} plain")};let mut row=Row::from([("title".into(),Cell::text_arc(title))]);if !single {row.insert("body".into(),Cell::text_arc(format!("body {i}")));}row
}).collect()}
fn main(){
 let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 let parts=[None,Some(""),Some("x"),Some("ЁЖ ΟΣ İ\u{b}note\u{a0}body"),Some("12345678901234567890123")];let fields=["title","body","snippet"];let mut rows=Vec::new();
 for a in parts {for b in parts {for c in parts {rows.push(fields.into_iter().zip([a,b,c]).filter_map(|(k,v)|v.map(|s|(k.to_string(),Cell::text_arc(s)))).collect::<Row>());}}}
 rows.push(Row::from([("body".into(),Cell::text_arc("wal note ".repeat(4096)))]));
 let mut scratch=String::new();for row in &rows {assert_eq!(stream_text(row,&mut scratch),row_embed_text(row).as_ref());}
 for dim in [8,9,128,768,1536] {let emb=HashingEmbedder::new("test",dim);assert!(exact(&baseline(&emb,&rows),&candidate(&emb,&rows)));}
 for(name,rows,loops)in[("default_1k",fixture(1000,false),16),("default_10k",fixture(10000,false),2),("single_field_1k",fixture(1000,true),16),("unicode_1k",(0..1000).map(|_|Row::from([("title".into(),Cell::text_arc("ЁЖ ΟΣ İ note".repeat(16))),("body".into(),Cell::text_arc("body\u{b}x"))])).collect(),4)]{
  let emb=HashingEmbedder::new("test",768);assert!(exact(&baseline(&emb,&rows),&candidate(&emb,&rows)));let mut ns=vec![Vec::<f64>::new();2];
  for round in 0..12 {for v in if (process+round)%2==0 {[0,1]}else{[1,0]} {
   let t=Instant::now();for _ in 0..loops {black_box(if v==0 {baseline(&emb,black_box(&rows))}else{candidate(&emb,black_box(&rows))});}
   ns[v].push(t.elapsed().as_nanos() as f64/loops as f64);
  }}println!("{}",serde_json::json!({"case":name,"rows":rows.len(),"ns":ns,"exact_values":true}));
 }
}
