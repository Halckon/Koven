//! Count parser work for valid and malformed adjacent-element separators.

#[test]
fn missing_separators_and_long_trivia_keep_dispatch_linear() {
    for gap in [" ".to_owned(), format!("/*{}*/", " ".repeat(1024))] {
        let make = |count| format!("{{ {} }}", format!("x{gap}").repeat(count));
        let (small_raw, small_iterations, small_visits, small_errors) =
            super::tests::block_dispatch_metrics(make(32));
        let (large_raw, large_iterations, large_visits, large_errors) =
            super::tests::block_dispatch_metrics(make(64));
        assert_eq!((small_errors, large_errors), (31, 63));
        assert!(small_iterations <= small_raw && large_iterations <= large_raw);
        assert!(large_visits <= small_visits * 2 + 64);
    }
}
