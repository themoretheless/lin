#[cfg(test)]
mod linux_keep_size_model {
    use super::*;
    #[test]
    fn bounded_geometry_never_reserves_historical_sparse_prefix() {
        const M: u64 = 1024 * 1024;
        assert_eq!(linux_wal_reservation_range(0, 0, 0, false), None);
        assert_eq!(linux_wal_reservation_range(0, M, 100, false), None);
        assert_eq!(linux_wal_reservation_range(6, 0, M, true), None);
        assert_eq!(linux_wal_reservation_range(6, 0, 1, false), Some((6, M as i64)));
        assert_eq!(linux_wal_reservation_range(6, 0, M + 1, false), Some((6, (2 * M) as i64)));
        assert_eq!(linux_wal_reservation_range(16 * 1024 * M, 0, 1, false), Some(((16 * 1024 * M) as i64, M as i64)));
        assert_eq!(linux_wal_reservation_range(6, 0, u64::MAX, false), None);
        assert_eq!(linux_wal_reservation_range(i64::MAX as u64, 0, 1, false), None);
    }
    #[test]
    fn independent_wide_arithmetic_boundary_model() {
        const M: u128 = 1024 * 1024;
        let logicals = [0, 1, 4095, 4096, M as u64 - 1, M as u64, M as u64 + 1, (1u64 << 32) - 1, 1u64 << 32, i64::MAX as u64 - M as u64, i64::MAX as u64, u64::MAX];
        let adds = [0, 1, 4095, M as u64 - 1, M as u64, M as u64 + 1, (16 * M) as u64, (16 * M + 13) as u64, (16 * M + 14) as u64, u32::MAX as u64, u64::MAX];
        let mut checked = 0;
        for logical in logicals {
            for additional in adds {
                for allocated in [0, logical, logical.saturating_add(additional), u64::MAX] {
                    for empty_only in [false, true] {
                        let want = if additional == 0 || additional as u128 > 16 * M + 13 || (empty_only && logical != 0) {
                            None
                        } else {
                            let required = logical as u128 + additional as u128;
                            let rounded = ((additional as u128 + M - 1) / M) * M;
                            if required > u64::MAX as u128 || allocated as u128 >= required || logical as u128 + rounded > i64::MAX as u128 {
                                None
                            } else {
                                Some((logical as i64, rounded as i64))
                            }
                        };
                        let got = linux_wal_reservation_range(logical, allocated, additional, empty_only);
                        assert_eq!(got, want, "{logical} {allocated} {additional} {empty_only}");
                        if let Some((offset, length)) = got {
                            assert_eq!(offset as u64, logical);
                            assert!(length as u128 >= additional as u128 && (length as u128) < additional as u128 + M);
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 1056);
    }
}

