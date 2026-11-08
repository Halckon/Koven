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

#[test]
fn damaged_nested_types_share_the_member_boundary_lookahead() {
    let metrics = |count| {
        let types = "I<".repeat(count);
        let trivia = "/* trivia */ ".repeat(count);
        let text = format!(
            "class Outer {{\nclass Inner: {types}A async\nborrow {trivia}fun next(): Unit {{}}\n}}"
        );
        super::parse_class_family_metrics(&text, count + 1)
    };
    let (small_raw, small_visits, _) = metrics(32);
    let (large_raw, large_visits, _) = metrics(64);
    assert!(small_visits <= small_raw * 24);
    assert!(large_visits <= large_raw * 24);
    assert!(
        large_visits <= small_visits * 2 + 64,
        "{large_visits} vs {small_visits}"
    );
}

#[test]
fn consecutive_poisoned_delegation_strings_use_linear_significant_visits() {
    let metrics = |count| {
        let entries = (0..count)
            .map(|index| format!(r#"I{index} by delegate "bad\q""#))
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = r#""bad\q" "#.repeat(count);
        let text = format!("class C: {entries}, Last by delegate {suffix}<A, J {{}}");
        super::parse_class_family_metrics(&text, 0)
    };
    let (small_raw, small_visits, _) = metrics(32);
    let (large_raw, large_visits, _) = metrics(64);
    assert!(small_visits <= small_raw * 24);
    assert!(large_visits <= large_raw * 24);
    assert!(large_visits <= small_visits * 2 + 64);
}

#[test]
fn unsupported_delegation_suffixes_use_linear_significant_visits() {
    let metrics = |count| {
        let entries = (0..count)
            .map(|index| format!("I{index} by delegate + Junk<A, B>([A, B], {{ -> 1 }})"))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!("class C: {entries}, J {{ fun keep(): Unit {{}} }}");
        super::parse_class_family_metrics(&text, count)
    };
    let (small_raw, small_visits, small_items) = metrics(32);
    let (large_raw, large_visits, large_items) = metrics(64);
    assert_eq!(small_items, 2);
    assert_eq!(large_items, 2);
    assert!(small_visits <= small_raw * 24);
    assert!(large_visits <= large_raw * 24);
    assert!(large_visits <= small_visits * 2 + 64);
}
