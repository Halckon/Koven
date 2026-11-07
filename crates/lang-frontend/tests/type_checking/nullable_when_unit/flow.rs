//! 原 nullable when 正常 flow 与非空读取消费者的双入口观察。
use super::checked_entries;

#[test]
fn nullable_when_null_fallthrough_narrows_else_subject() {
    // Removing null from the remaining domain must make the same stable subject usable as T.
    let (_, _, entries) =
        checked_entries("fun extract(x: Int?): Int = when (x) { null -> 0; else -> x }");
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.non_null_uses().len(), 1);
    }
}

#[test]
fn nullable_when_remaining_domain_covers_boolean_and_enum_orders() {
    // Both null position and comma alternatives affect the domain reaching each body.
    for text in [
        "fun extract(x: Boolean?): Boolean = when (x) { null -> false; true -> x; false -> x }",
        "fun extract(x: Boolean?): Boolean = when (x) { true, false -> x; null -> false }",
        "fun extract(x: Boolean?): Boolean = when (x) { true -> x; null -> false; else -> x }",
        "enum class Flag { On, Off }\nfun extract(x: Flag?): Flag = when (x) { null -> Flag.Off; else -> x }",
    ] {
        let (_, _, entries) = checked_entries(text);
        for (_, typed) in entries {
            assert!(
                typed.diagnostics().is_empty(),
                "{text}: {:?}",
                typed.diagnostics()
            );
        }
    }
}

#[test]
fn nullable_when_identity_does_not_interfere_with_later_if_refinement() {
    let text = "fun extract(x: Boolean?): Boolean {
        when (x) { null, true, false -> 0 }
        return (if (x != null) { x } else { false })
    }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    }
}

#[test]
fn nullable_when_identity_does_not_interfere_with_body_if_refinement() {
    let text = "fun extract(x: Boolean?): Boolean = when (x) {
        null, true, false -> if (x != null) { x } else { false }
    }";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    }
}
