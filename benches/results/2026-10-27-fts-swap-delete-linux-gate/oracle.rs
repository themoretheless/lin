#[cfg(test)]
mod fused_swap_delete_oracle {
    use super::*;
    use crate::store::Cell;
    fn row(i: usize) -> Row {
        Row::from([
            ("title".into(), Cell::text_arc(format!("SHARED word{} ЁЖ", i % 5))),
            ("body".into(), Cell::text_arc(format!("body{} shared", i % 7))),
            ("snippet".into(), Cell::text_arc(if i % 3 == 0 {"needle shared"} else {""})),
        ])
    }
    fn verify(index: &FtsIndex, rows: &[Row], fields: &[String]) {
        let mut actual = index.clone(); actual.fold_all();
        let expected = FtsIndex::build(rows, fields);
        assert_eq!(actual.postings, expected.postings);
        for query in ["shared", "ёж", "needle", "word0", "word4", "body0", "missing"] {
            assert_eq!(index.candidate_idxs(query), expected.candidate_idxs(query));
        }
    }
    #[test]
    fn pending_edits_and_large_token_fallback_match_rebuild() {
        let fields = vec!["title".into(), "body".into()];
        for large in 0..3 {
            let mut rows = (0..40).map(row).collect::<Vec<_>>();
            if large == 1 {rows[0].insert("body".into(), Cell::text_arc("X".repeat(100_000)));}
            if large == 2 {rows[0].insert("body".into(), Cell::text_arc((0..2500).map(|i|format!("token{i}")).collect::<Vec<_>>().join(" ")));}
            let mut index = FtsIndex::build(&rows, &fields);
            let mut state = 17u64;
            for step in 0..256 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                if step % 3 == 0 {
                    let p = state as usize % rows.len(); let after = row(step + 1000);
                    index.sync_row(p, &rows[p], &after); rows[p] = after;
                }
                let p = if step == 0 {0} else if step % 8 == 0 {rows.len()-1} else {state as usize % rows.len()};
                let last = rows.len()-1; let removed = rows.swap_remove(p);
                if p == last {index.remove_row(p, &removed);} else {index.swap_remove_row(p,last,&removed,&rows[p]);}
                verify(&index,&rows,&fields);
                let next = row(step+2000);index.insert_row(rows.len(),&next);rows.push(next);
                if step % 37 == 0 {index.fold_all();}
                verify(&index,&rows,&fields);
            }
            while !rows.is_empty() {
                let p = rows.len()/2;let last=rows.len()-1;let removed=rows.swap_remove(p);
                if p==last {index.remove_row(p,&removed);}else{index.swap_remove_row(p,last,&removed,&rows[p]);}
                verify(&index,&rows,&fields);
            }
        }
    }
}
