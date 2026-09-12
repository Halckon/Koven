//! SPEC-0208：常量读取必须复用普通值的所有权路径。

use lang_frontend::{
    name_resolution::resolve_names,
    ownership_checking::{
        ConstantMaterializationKind, DropPoint, DropTarget, LoanTarget, OwnershipCheckedFile,
        check_ownership,
    },
    source::SourceMap,
    type_checking::{TypedFile, check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

fn analyzed(text: &str) -> (SourceMap, TypedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("constants.ko", text).expect("source");
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "constants");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.constants().is_some());
    let ownership = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    (sources, typed, ownership)
}

fn checked(text: &str) -> (SourceMap, OwnershipCheckedFile) {
    let (sources, typed, ownership) = analyzed(text);
    assert!(
        ownership.diagnostics().is_empty(),
        "{:?}",
        ownership.diagnostics()
    );
    assert!(
        ownership.deferred().is_empty(),
        "{:?}",
        ownership.deferred()
    );
    assert!(
        ownership
            .constant_materializations()
            .expect("validated materializations")
            .matches(&typed)
    );
    (sources, ownership)
}

#[test]
fn repeated_string_borrows_own_distinct_temporaries_without_declaration_loans() {
    let (_, ownership) = checked(
        r#"
object Labels { const val TEXT = "hello" }
const val COPY = Labels.TEXT
fun view(text: String): Unit {}
fun use(): Unit {
    val first = view(COPY)
    val second = view(COPY)
    val third = view(Labels.TEXT)
}
"#,
    );
    // Dependency reads have no runtime effects; each executed read has its own owner.
    assert_eq!(ownership.loans().len(), 3);
    let mut owners = Vec::new();
    for loan in ownership.loans() {
        let LoanTarget::Temporary(owner) = loan.target() else {
            panic!("constant declaration must not be borrowed: {loan:?}");
        };
        owners.push(*owner);
        assert_eq!(
            ownership
                .drops()
                .iter()
                .filter(|drop| {
                    drop.target() == DropTarget::Temporary(*owner)
                        && drop.point() == DropPoint::CallReturn(loan.call())
                })
                .count(),
            1
        );
    }
    owners.sort_by_key(|owner| owner.index());
    owners.dedup();
    assert_eq!(owners.len(), 3);
    assert_eq!(ownership.drops().len(), 3);
    assert!(ownership.captures().is_empty());
}

#[test]
fn string_binary_operands_drop_materializations_in_reverse_order() {
    let (sources, ownership) = checked(
        r#"
const val TEXT = "hello"
fun use(): Boolean = (TEXT + TEXT) == TEXT
"#,
    );
    assert_eq!(ownership.drops().len(), 4);
    assert!(ownership.drops().iter().all(|drop| {
        matches!(drop.target(), DropTarget::Temporary(_))
            && matches!(drop.point(), DropPoint::AfterBinaryOperands(_))
    }));
    let drops = ownership.drops();
    assert_eq!(drops[0].point(), drops[1].point());
    assert!(drops[0].value_origin().start() > drops[1].value_origin().start());
    assert_eq!(sources.slice(drops[0].value_origin()).unwrap(), "TEXT");
    assert_eq!(sources.slice(drops[1].value_origin()).unwrap(), "TEXT");
}

#[test]
fn scalar_delivery_and_string_return_do_not_own_constant_declarations() {
    let (_, ownership) = checked(
        r#"
const val NUMBER = 7
const val LETTER = '文'
const val TEXT = "hello"
fun take(own number: Int, own letter: Char): Unit {}
fun use(): Unit {
    val first = take(NUMBER, LETTER)
    val second = take(NUMBER, LETTER)
}
fun first(): String = TEXT
fun second(): String = TEXT
"#,
    );
    assert!(ownership.loans().is_empty());
    assert!(ownership.drops().is_empty());
    assert!(ownership.captures().is_empty());
}

#[test]
fn closure_reads_do_not_capture_constant_or_namespace_identity() {
    let (_, ownership) = checked(
        r#"
object Labels { const val TEXT = "hello" }
class Names { companion object { const val TEXT = Labels.TEXT } }
fun use(): Unit {
    val shared: () -> String = { Labels.TEXT }
    val owned: move () -> String = move { Names.TEXT }
}
"#,
    );
    assert!(ownership.captures().is_empty());
    assert!(ownership.loans().is_empty());
}

#[test]
fn materialization_plans_exclude_dependencies_and_bind_exact_analysis() {
    let source = r#"
const val TEXT = "hi"
const val COPY = TEXT
const val NUMBER = 7
fun stop(): Nothing = stop()
fun forever(): Unit {
    loop { continue }
    val unreachable = view(TEXT)
}
fun aborted(): Unit {
    val stopped = stop()
    val unreachable = view(TEXT)
}
fun view(text: String): Unit {}
fun use(): Int {
    val first = view(COPY)
    val second = view(TEXT)
    return NUMBER
    val unreachable = view(TEXT)
}
"#;
    let (sources, typed, owned) = analyzed(source);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let facts = owned.constant_materializations().expect("validated");
    assert!(facts.matches(&typed));
    assert_eq!(facts.plans().len(), 3);
    assert_eq!(owned.drops().len(), 2);
    let (_, other, repeated) = analyzed(source);
    assert!(!facts.matches(&other));
    let snapshot = |owned: &OwnershipCheckedFile| {
        owned
            .constant_materializations()
            .unwrap()
            .plans()
            .iter()
            .map(|plan| {
                let descriptor = plan.descriptor();
                (
                    descriptor.expression().index(),
                    descriptor.target().index(),
                    descriptor.value().clone(),
                    plan.kind(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(snapshot(&owned), snapshot(&repeated));
    for (plan, kind) in facts.plans().iter().zip([
        ConstantMaterializationKind::StringTemporary,
        ConstantMaterializationKind::StringTemporary,
        ConstantMaterializationKind::InlineCopy,
    ]) {
        assert_eq!(plan.kind(), kind);
        assert_eq!(
            facts
                .plan_at(plan.descriptor().expression())
                .unwrap()
                .descriptor()
                .target(),
            plan.descriptor().target()
        );
    }
    assert_eq!(
        sources.slice(owned.drops()[0].value_origin()).unwrap(),
        "COPY"
    );
}

#[test]
fn ownership_failure_clears_materialization_capability() {
    let (_, _, owned) = analyzed(
        r#"
const val TEXT = "hi"
fun take(own text: String): Unit {}
fun use(own input: String): Unit {
    val first = take(TEXT)
    val second = take(input)
    val third = take(input)
}
"#,
    );
    assert!(!owned.diagnostics().is_empty());
    assert!(owned.constant_materializations().is_none());
    assert!(owned.drops().is_empty());
    assert!(owned.loans().is_empty());
}

#[test]
fn constant_cleanup_matches_literal_on_control_flow_edges() {
    for body in [
        "val result = view(VALUE, if (flag) { return } else { 0 })",
        "val result = view(VALUE, if (flag) { stop() } else { 0 })",
        "loop { val result = view(VALUE, if (flag) { break } else { 0 })\nbreak }",
        "loop { val result = view(VALUE, if (flag) { continue } else { 0 })\nbreak }",
        "val result = view(if (flag) { VALUE } else { VALUE }, 0)",
        "val result = take(VALUE, if (flag) { return } else { 0 })",
        "val result = take(VALUE, if (flag) { stop() } else { 0 })",
    ] {
        let source = format!(
            "const val TEXT = \"hi\"\nfun stop(): Nothing = stop()\nfun view(text: String, number: Int): Unit {{}}\nfun take(own text: String, number: Int): Unit {{}}\nfun use(flag: Boolean): Unit {{ {body} }}"
        );
        let (constant_sources, constant) = checked(&source.replace("VALUE", "TEXT"));
        let (literal_sources, literal) = checked(&source.replace("VALUE", "\"hi\""));
        // Compare actual cleanup boundaries and owner origins, without comparing arena IDs.
        let drops = |sources: &SourceMap, owned: &OwnershipCheckedFile| {
            owned
                .drops()
                .iter()
                .map(|drop| {
                    (
                        std::mem::discriminant(&drop.point()),
                        std::mem::discriminant(&drop.target()),
                        sources
                            .slice(drop.value_origin())
                            .unwrap()
                            .replace("TEXT", "\"hi\""),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            drops(&constant_sources, &constant),
            drops(&literal_sources, &literal),
            "{body}"
        );
        assert_eq!(constant.loans().len(), literal.loans().len(), "{body}");
        assert_eq!(
            constant
                .loan_ends()
                .iter()
                .map(|end| std::mem::discriminant(&end.point()))
                .collect::<Vec<_>>(),
            literal
                .loan_ends()
                .iter()
                .map(|end| std::mem::discriminant(&end.point()))
                .collect::<Vec<_>>(),
            "{body}"
        );
        assert!(
            !constant
                .constant_materializations()
                .unwrap()
                .plans()
                .is_empty(),
            "{body}"
        );
        assert!(
            literal
                .constant_materializations()
                .unwrap()
                .plans()
                .is_empty()
        );
    }
}
