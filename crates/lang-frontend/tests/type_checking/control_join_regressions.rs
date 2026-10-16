//! Ordinary control-join regression cases, kept separate from the legacy test inventory.
use super::*;

#[test]
fn subjectless_when_requires_boolean_conditions_and_runtime_any_is_not_a_join_target() {
    let invalid = "fun invalid(): Int = when { 1 -> 1; else -> 0 }";
    let (sources, _, _, typed) = checked(invalid);
    assert_eq!(codes(typed.diagnostics()), ["L0107"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .expect("condition span"),
        "1"
    );

    let runtime_any = "fun mixed(flag: Boolean): Any = when (flag) {
                     true -> 1
                     false -> \"text\"
                 }";
    let (sources, _, _, typed) = checked(runtime_any);
    assert_eq!(codes(typed.diagnostics()), ["L0112"]);
    assert_eq!(
        sources
            .slice(typed.diagnostics()[0].primary_span())
            .unwrap(),
        "\"text\""
    );
}

#[test]
fn when_branch_join_handles_nothing_nullable_enum_cases_and_rejects_mixed() {
    let text = "enum class Choice { First, Second }
                fun joins(flag: Boolean, optional: Int?, choice: Choice): Unit {
                    val nullable = when (flag) { true -> 1; false -> optional }
                    val root = when (choice) {
                        is Choice.First -> choice
                        is Choice.Second -> choice
                    }
                    val mixed = when (flag) { true -> 1; false -> \"text\" }
                }
                fun bottom(flag: Boolean): Int = when (flag) {
                    true -> return 1
                    false -> 2
                }";
    let (_, _, names, typed) = checked(text);
    assert_eq!(codes(typed.diagnostics()), ["L0112"]);

    let symbol_type = |name: &str| {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == name)
            .expect("local symbol");
        typed
            .symbol_type(symbol.id())
            .and_then(|ty| typed.types().get(ty))
            .expect("local type")
    };
    assert!(matches!(
        symbol_type("nullable"),
        TypeKind::Nullable(inner)
            if matches!(typed.types().get(*inner), Some(TypeKind::Builtin(BuiltinType::Int)))
    ));
    assert!(matches!(symbol_type("root"), TypeKind::Nominal { .. }));
    assert!(matches!(symbol_type("mixed"), TypeKind::Error));
}
