use lin::{Cell, Row};
use std::collections::BTreeMap;
#[test]
fn public_row_retains_exact_btreemap_contract() {
    let identity: fn(BTreeMap<String, Cell>) -> Row = std::convert::identity::<BTreeMap<String, Cell>>;
    let row = identity(BTreeMap::from([("key".to_owned(), Cell::Int(7))]));
    let same: &BTreeMap<String, Cell> = &row;
    assert_eq!(same["key"], Cell::Int(7));
    let owned: BTreeMap<String, Cell> = row;
    assert_eq!(owned.into_keys().collect::<Vec<_>>(), ["key"]);
    assert_eq!(std::mem::size_of::<Row>(), std::mem::size_of::<BTreeMap<String, Cell>>());
}
