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
