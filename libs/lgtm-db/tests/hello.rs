//! Public placeholder database contract.

// tests -----------------------------------------------------------------------

#[test]
fn should_supply_hello_without_persistence() {
    let database = lgtm_db::Database;
    assert_eq!(database.get_hello(), "Hello");
}
