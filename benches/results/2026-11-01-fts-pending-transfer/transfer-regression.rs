    #[test]
    fn singleton_pending_transfer_reuses_storage_and_merges_sorted() {
        let mut from = FxHashMap::default();
        let mut to = FxHashMap::default();
        let key = "transfer-term".to_owned();
        let key_ptr = key.as_ptr();
        let list = vec![7usize];
        let list_ptr = list.as_ptr();
        from.insert(key, list);
        assert!(transfer_single_pending(&mut from, &mut to, "transfer-term", 7));
        assert!(from.is_empty());
        let (key, list) = to.get_key_value("transfer-term").unwrap();
        assert_eq!(key.as_ptr(), key_ptr);
        assert_eq!(list.as_ptr(), list_ptr);
        assert_eq!(list, &[7]);
        assert!(transfer_single_pending(&mut to, &mut from, "transfer-term", 7));
        to.insert("transfer-term".to_owned(), vec![2, 9]);
        assert!(transfer_single_pending(&mut from, &mut to, "transfer-term", 7));
        assert_eq!(to["transfer-term"], vec![2, 7, 9]);
        from.insert("transfer-term".to_owned(), vec![7]);
        assert!(transfer_single_pending(&mut from, &mut to, "transfer-term", 7));
        assert_eq!(to["transfer-term"], vec![2, 7, 9]);
        from.insert("transfer-term".to_owned(), vec![7, 8]);
        let before = from.clone();
        let dest_before = to.clone();
        assert!(!transfer_single_pending(&mut from, &mut to, "transfer-term", 7));
        assert_eq!(from, before);
        assert_eq!(to, dest_before);
        assert!(!transfer_single_pending(&mut from, &mut to, "missing", 7));
        from.insert("other".to_owned(), vec![8]);
        assert!(!transfer_single_pending(&mut from, &mut to, "other", 7));
        assert_eq!(from["other"], vec![8]);
    }
