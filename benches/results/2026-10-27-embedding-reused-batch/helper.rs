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

pub fn diagnostic_embedding_baseline(emb:&HashingEmbedder,rows:&[crate::store::Row])->Vec<Arc<[f32]>> {
 let texts:Vec<_>=rows.iter().map(row_embed_text).collect();let refs:Vec<&str>=texts.iter().map(|t|t.as_ref()).collect();emb.embed_batch(&refs)
}
pub fn diagnostic_embedding_reused_batch(emb:&HashingEmbedder,rows:&[crate::store::Row])->Vec<Arc<[f32]>> {
 EMBED_SCRATCH.with(|cell| {
  let mut guard=cell.borrow_mut();let(scratch,lower)=&mut *guard;scratch.ensure_dim(emb.dim);
  let mut text=String::with_capacity(256);let mut out=Vec::with_capacity(rows.len());
  for row in rows {let joined=stream_text(row,&mut text);emb.fill(scratch,crate::fts::lowercased(joined,lower));out.push(scratch.finish());}
  out
 })
}
