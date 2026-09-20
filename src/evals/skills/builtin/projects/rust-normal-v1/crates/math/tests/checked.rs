#[test]
fn overflow_is_explicit() { assert_eq!(fixture_math::add(u32::MAX, 1), None); }
