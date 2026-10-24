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
