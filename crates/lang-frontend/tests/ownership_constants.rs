//! SPEC-0208：常量读取必须复用普通值的所有权路径。

use lang_frontend::{
    name_resolution::resolve_names,
    ownership_checking::{
        DropPoint, DropTarget, LoanTarget, OwnershipCheckedFile, check_ownership,
    },
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

fn checked(text: &str) -> (SourceMap, OwnershipCheckedFile) {
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
