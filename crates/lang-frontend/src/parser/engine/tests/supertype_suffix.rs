#[test]
fn poisoned_supertype_suffixes_keep_linear_significant_token_visits() {
    let metrics = |count| {
        let entries = (0..count)
            .map(|index| format!("I{index} async Junk<A, B> (X, {{ -> 1 }})"))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!("class C: {entries} {{}}");
        super::parse_class_family_metrics(&text, 0)
    };
    let (small_raw, small_visits, _) = metrics(32);
    let (large_raw, large_visits, _) = metrics(64);
    assert!(small_visits <= small_raw * 24);
    assert!(large_visits <= large_raw * 24);
    assert!(large_visits <= small_visits * 2 + 64);
}
