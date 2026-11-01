//! Same-analysis source-qualified identities pass through the production final gate.
use super::*;

#[test]
fn formed_normalization_preserves_distinct_sources_and_removes_only_exact_duplicates() {
    let a = "package a\nfun run(): Unit { val seed = \"a\"; val tail = \"b\" }";
    let b = "package b\nfun run(): Unit { val seed = \"a\"; val tail = \"b\" }";
    let fixture = Fixture::new(&[("a/history.ko", a), ("b/history.ko", b)]);
    let published = fixture.shared();
    assert!(published.diagnostics().is_empty());
    assert!(published.deferred().is_empty());
    let left = fixture.local(0, "seed");
    let right = fixture.local(1, "seed");
    assert_eq!(
        left.symbol(),
        right.symbol(),
        "same local index in distinct sources"
    );
    assert_ne!(left.source_unit(), right.source_unit());
    assert_ne!(left, right);
    let mut expected = vec![
        left,
        fixture.local(0, "tail"),
        right,
        fixture.local(1, "tail"),
    ];
    expected.sort();
    assert_eq!(published.formed_owned_locals(), expected);
    let mut history = expected.iter().rev().copied().collect::<Vec<_>>();
    history.extend_from_slice(&expected);
    assert_eq!(history.len(), 8);
    assert!(history.windows(2).any(|pair| pair[0] > pair[1]));
    assert!(super::super::formed_publication::finalize(
        fixture.typed.types(),
        &[],
        &[],
        &mut history,
    ));
    assert_eq!(history, expected);
    assert!(super::super::formed_publication::finalize(
        fixture.typed.types(),
        &[],
        &[],
        &mut history,
    ));
    assert_eq!(history, expected, "normalization is idempotent");
}
