//! Allocation mechanism proof, not a throughput benchmark.
use lin::{Cell, Row};
use std::{alloc::{GlobalAlloc, Layout, System}, cell::Cell as Counter, collections::BTreeMap, hint::black_box};
thread_local! {
    static COUNTING: Counter<bool> = const { Counter::new(false) };
    static ALLOCATIONS: Counter<usize> = const { Counter::new(0) };
}
struct CountingAllocator;
fn record() {
    let _ = COUNTING.try_with(|enabled| {
        if enabled.get() { let _ = ALLOCATIONS.try_with(|n| n.set(n.get()+1)); }
    });
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 { record(); unsafe { System.alloc(layout) } }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 { record(); unsafe { System.alloc_zeroed(layout) } }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 { record(); unsafe { System.realloc(ptr, layout, size) } }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { unsafe { System.dealloc(ptr, layout) } }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
fn measured<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ALLOCATIONS.with(|n| n.set(0));
    COUNTING.with(|enabled| { assert!(!enabled.replace(true)); });
    let value = black_box(f());
    COUNTING.with(|enabled| enabled.set(false));
    let count = ALLOCATIONS.with(Counter::get);
    (value, count)
}
#[test]
fn cloned_template_eliminates_each_field_name_allocation() {
    for long in [false, true] {
        let legacy: BTreeMap<String, Cell> = (0..12).map(|i| {
            let key = if long { format!("custom_metadata_field_with_long_name_{i:02}") } else { format!("field_{i:02}") };
            (key, Cell::Int(i))
        }).collect();
        let row = Row::from(legacy.clone());
        for _ in 0..3 {
            let (old, old_allocations) = measured(|| black_box(&legacy).clone());
            let (new, new_allocations) = measured(|| black_box(&row).clone());
            assert_eq!(old, new.clone().into_iter().collect::<BTreeMap<String, Cell>>());
            assert_eq!(old_allocations.checked_sub(new_allocations), Some(legacy.len()), "long={long}, old={old_allocations}, new={new_allocations}");
            println!("long={long}: legacy={old_allocations}, row_key={new_allocations}, eliminated={}", old_allocations-new_allocations);
        }
    }
}

#[test]
fn borrowed_field_insertion_eliminates_temporary_string_allocation() {
    for name in ["id", "hash", "embedding", "a custom metadata field name longer than inline capacity"] {
        let (owned, old) = measured(|| {
            let mut row = Row::new();
            row.insert(black_box(name).to_owned(), Cell::Int(42));
            row
        });
        let (borrowed, new) = measured(|| {
            let mut row = Row::new();
            row.insert_str(black_box(name), Cell::Int(42));
            row
        });
        assert_eq!(owned, borrowed);
        assert_eq!(old.checked_sub(new), Some(1), "name={name}, owned={old}, borrowed={new}");
        println!("insert {name}: owned={old}, borrowed={new}");
    }
}
