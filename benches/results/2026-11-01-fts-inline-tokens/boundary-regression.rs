    #[test]
    fn inline_tokens_spill_preserves_terms_and_posting_edits() {
        let fields = vec!["title".to_owned(), "body".to_owned()];
        for n in [0, 1, 7, 8, 9, 17, 64] {
            let words: Vec<String> = (0..n).map(|i| format!("term{i:03}")).collect();
            let text = words.join(" ");
            let before = Row::from([("title".into(), Cell::text_arc(text.as_str()))]);
            let tokens = row_tokens(&before, &fields);
            assert_eq!(tokens.iter().map(|s| s.as_ref()).collect::<Vec<_>>(), words.iter().map(String::as_str).collect::<Vec<_>>());
            assert_eq!(tokens.spilled(), n > 8);
            let after = Row::from([("title".into(), Cell::text_arc("ЁЖ WAL wal")), ("body".into(), Cell::text_arc(text.as_str()))]);
            let mut index = FtsIndex::empty(&fields);
            index.insert_row(0, &before);
            index.sync_row(0, &before, &after);
            for word in &words { assert_eq!(index.candidate_idxs(word), vec![0]); }
            assert_eq!(index.candidate_idxs("ёж wal"), vec![0]);
            index.remove_row(0, &after);
            for word in &words { assert!(index.candidate_idxs(word).is_empty()); }
            assert!(index.candidate_idxs("ёж wal").is_empty());
        }
    }
