//! SPEC-0312: real ordinary source, no Map and no synthetic facts.
#[path = "lower_frontend_owned_source_structure_tests.rs"]
mod structure;
use super::owned_source_return_tests::unit_contract::{self, Control, Expected};
use lang_frontend::{
    diagnostic::{Diagnostic, Severity, codes},
    lexer::lex,
    name_resolution::{NameResolution, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{SequentialContainerKind, TypedFile, check_types, standard_environments},
};

// Keep the historical Single comparison local to the two diagnostics that still use it.
struct Analysis {
    sources: SourceMap,
    parsed: ParsedFile,
    names: NameResolution,
    typed: TypedFile,
    owned: OwnershipCheckedFile,
}

fn analyze(text: &str) -> Analysis {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("lowering.ko", text)
        .expect("source must be unique");
    let lexed = lex(&sources, source).expect("lexing must succeed internally");
    let parsed = parse_file(&sources, &lexed).expect("parsing must succeed internally");
    let (environment, types) = standard_environments();
    let names =
        resolve_names(&sources, &parsed, &environment).expect("names must resolve internally");
    let typed =
        check_types(&sources, &parsed, &names, &types).expect("types must check internally");
    let owned = check_ownership(&sources, &parsed, &names, &typed)
        .expect("ownership must check internally");
    Analysis {
        sources,
        parsed,
        names,
        typed,
        owned,
    }
}

struct ExpectedDiagnostic<'a> {
    code: &'a str,
    message: &'a str,
    primary: (usize, usize, &'a str),
    labels: &'a [(usize, usize, &'a str, &'a str)],
}

// Complete ordinary-source observations frozen on main 94768a91 before this comparison.
// Construct every expected field explicitly, including the absence of notes/help.
fn assert_complete_diagnostic(
    text: &str,
    sources: &SourceMap,
    actual: &[Diagnostic],
    expected: ExpectedDiagnostic<'_>,
) {
    let [diagnostic] = actual else {
        panic!("exactly one original ownership diagnostic");
    };
    let source = diagnostic.primary_span().source_id();
    assert_eq!(sources.source_name(source), Ok("lowering.ko"));
    assert_eq!(sources.source_text(source), Ok(text));
    let (start, end, spelling) = expected.primary;
    let primary = sources.span(source, start, end).unwrap();
    assert_eq!(sources.slice(primary), Ok(spelling));
    let mut complete = Diagnostic::new(
        sources,
        Severity::Error,
        codes::catalog().unwrap().resolve(expected.code).unwrap(),
        expected.message,
        primary,
    )
    .unwrap();
    for &(start, end, spelling, message) in expected.labels {
        let span = sources.span(source, start, end).unwrap();
        assert_eq!(sources.slice(span), Ok(spelling));
        complete.add_label(sources, span, message).unwrap();
    }
    crate::test_support::diagnostics::assert_eq_diagnostic!(
        actual,
        [complete],
        "complete original diagnostic, source identity and ordered details"
    );
}

// Unit has the same rejection and source locations, but more specific conflict messages
// and selected-parameter context. Keep the complete Legacy oracle beside this one.
fn assert_unit_diagnostic(text: &str, expected: ExpectedDiagnostic<'_>) {
    use crate::native_tests::boxed_enum_tests::unit_source::{SourceRejectionKind, UnitSource};

    let rejected = UnitSource::analyze("lowering.ko", text)
        .err()
        .expect("an original illegal source cannot acquire executable Unit capability");
    assert!(matches!(rejected.kind, SourceRejectionKind::Diagnostics));
    assert_eq!(
        rejected.report.stages,
        [("parse", 0), ("names", 0), ("types", 0), ("ownership", 1)]
    );
    let [(stage, diagnostic)] = rejected.report.diagnostics.as_slice() else {
        panic!("exactly the original ownership rejection");
    };
    assert_eq!(*stage, "ownership");
    assert_complete_diagnostic(
        text,
        &rejected.sources,
        std::slice::from_ref(diagnostic),
        expected,
    );
}

fn check(live: bool, factory: &str) {
    check_with_decoy(live, factory, false);
}

fn check_with_decoy(live: bool, factory: &str, decoy: bool) {
    let text = format!(
        "fun main(): Unit {{ val keys = {factory}(1, 2, 3); for (key in keys) {{ {decoy_body}if (key == 2) {{ continue }} }}; {} }}",
        if live { "keys.size" } else { "" },
        decoy_body = if decoy {
            format!("val other = {factory}(9); other.size; ")
        } else {
            String::new()
        }
    );
    let container = match factory {
        "arrayOf" => SequentialContainerKind::Array,
        "listOf" => SequentialContainerKind::List,
        "mutableListOf" => SequentialContainerKind::MutableList,
        _ => panic!("original owned-source factory"),
    };
    let (program, origins) = unit_contract::observe(
        &text,
        "main",
        &[Expected {
            function: "main",
            live,
            container,
            own: false,
        }],
        Control::Continue,
        false,
    );
    let [origin] = origins.as_slice() else {
        panic!("one original provider");
    };
    structure::check(&program, live, origin.seed.unwrap());
}

#[test]
fn owned_source_continue_array_live_without_map() {
    check(true, "arrayOf");
}
#[test]
fn owned_source_continue_array_dead_without_map() {
    check(false, "arrayOf");
}
#[test]
fn owned_source_continue_list_live_without_map() {
    check(true, "listOf");
}
#[test]
fn owned_source_continue_list_dead_without_map() {
    check(false, "listOf");
}
#[test]
fn owned_source_continue_mutable_list_live_without_map() {
    check(true, "mutableListOf");
}
#[test]
fn owned_source_continue_mutable_list_dead_without_map() {
    check(false, "mutableListOf");
}
fn assert_unit_boundary(live: bool, constants: bool) {
    use crate::ssa::unit_lower_test_support::{declaration, parsed};
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{
            check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        },
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let text = format!(
        "package p\nfun main(): Unit {{ val keys = arrayOf(1, 2, 3); for (key in keys) {{ if (key == 2) {{ continue }} }}; {} }}",
        if live { "keys.size" } else { "" }
    );
    let mut sources = SourceMap::new();
    let (source_id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source_id, &file)];
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let result = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let entry = declaration(&names, "p", "main");
    let result = if constants {
        let typed = result.validate_constants().unwrap();
        let owned =
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap();
        crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    } else {
        let typed = result.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    };
    let (program, entry) =
        result.unwrap_or_else(|error| panic!("unit constants={constants}, live={live}: {error:?}"));
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
}
#[test]
fn owned_source_continue_unit_basic_dead() {
    assert_unit_boundary(false, false);
}
#[test]
fn owned_source_continue_unit_basic_live() {
    assert_unit_boundary(true, false);
}
#[test]
fn owned_source_continue_unit_constant_dead() {
    assert_unit_boundary(false, true);
}
#[test]
fn owned_source_continue_unit_constant_live() {
    assert_unit_boundary(true, true);
}

// Exact outcomes; the original baseline observations remain outside the repository.
#[test]
fn owned_source_continue_nested_scope_boundaries() {
    for (_label, text) in [
        (
            "inner_fresh",
            "fun main(): Unit { for (outer in arrayOf(1, 2)) { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { continue } }; keys.size } }",
        ),
        (
            "outer_independent_inner",
            "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { for (other in arrayOf(3)) { other }; if (key == 2) { continue } }; keys.size }",
        ),
        (
            "same_source_nested",
            "fun main(): Unit { val keys = arrayOf(1, 2); for (outer in keys) { for (key in keys) { if (key == 2) { continue } } }; keys.size }",
        ),
        (
            "sequential_same_source",
            "fun main(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { continue } }; for (key in keys) { if (key == 2) { continue } }; keys.size }",
        ),
    ] {
        // All original nested/sequential fixtures retain their supported Unit contract.
        use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
        use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

        let unit = UnitSource::analyze("lowering.ko", text)
            .expect("strict unchanged nested-loop Unit admission");
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        assert_eq!(unit.parsed().roots().len(), 1);
        let [document] = unit.names().names().index().source_units() else {
            panic!("one original MemoryDocument");
        };
        assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
        assert_eq!(document.source_id(), unit.parsed().source_id());
        assert_eq!(
            unit.sources().source_name(document.source_id()),
            Ok("lowering.ko")
        );
        assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
        assert_eq!(unit.basic_types().types().sequential_iterations().len(), 2);
        assert_eq!(unit.ownership().iterations().len(), 2);
        for plan in unit.ownership().iterations() {
            let descriptor = plan.descriptor();
            assert_eq!(descriptor.statement().source_unit(), document.id());
            assert_eq!(
                unit.types().sequential_iteration(descriptor.statement()),
                Some(descriptor)
            );
        }
        let declaration = unit.declaration("main", SymbolKind::Function);
        assert_eq!(
            unit.sources().slice(unit.declaration_span(declaration)),
            Ok(text)
        );
        let (program, entry) = unit
            .lower_entry(declaration)
            .expect("the same nested or sequential loops lower through Unit");
        let [module] = program.modules.as_slice() else {
            panic!("one original source module");
        };
        let [function] = module.functions.as_slice() else {
            panic!("only the original main root is lowered");
        };
        assert_eq!(function.id(), entry);
        assert_eq!(
            function.origin,
            crate::ssa::model::Origin::Source(unit.declaration_span(declaration))
        );
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|instruction| matches!(
                    instruction.operation,
                    crate::ssa::model::Operation::ContainerElementPlace { .. }
                ))
                .count(),
            2,
            "both original loops retain element access"
        );
        crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("the actual Unit entry and complete two-loop SSA verify through LLVM");
    }
}

#[test]
fn owned_source_continue_frontend_source_loan_blocks_move_replace_and_mutation() {
    for (factory, operation, primary, loan) in [
        ("arrayOf", "consume(keys)", (119, 123, "keys"), (103, 107)),
        ("arrayOf", "keys[0] = 9", (111, 118, "keys[0]"), (103, 107)),
        (
            "mutableListOf",
            "keys.add(4)",
            (117, 121, "keys"),
            (109, 113),
        ),
        (
            "mutableListOf",
            "keys.removeLast()",
            (117, 121, "keys"),
            (109, 113),
        ),
    ] {
        let text = format!(
            "fun consume(own keys: Array<Int>): Unit {{}}\nfun main(): Unit {{ var keys = {factory}(1, 2, 3); for (key in keys) {{ {operation}; if (key == 2) {{ continue }} }} }}"
        );
        let analysis = analyze(&text);
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert_complete_diagnostic(
            &text,
            &analysis.sources,
            analysis.owned.diagnostics(),
            ExpectedDiagnostic {
                code: "L0135",
                message: "access conflicts with the active iteration source loan",
                primary,
                labels: &[(loan.0, loan.1, "keys", "conflicting loan starts here")],
            },
        );
        let message = match operation {
            "consume(keys)" => "move conflicts with an active loan",
            "keys[0] = 9" => "mutation conflicts with an active loan",
            "keys.add(4)" | "keys.removeLast()" => "new loan conflicts with an active loan",
            _ => panic!("one frozen original operation"),
        };
        let mut labels = vec![(loan.0, loan.1, "keys", "conflicting loan starts here")];
        if operation == "consume(keys)" {
            labels.push((
                12,
                32,
                "own keys: Array<Int>",
                "selected parameter declared here",
            ));
        }
        assert_unit_diagnostic(
            &text,
            ExpectedDiagnostic {
                code: "L0135",
                message,
                primary,
                labels: &labels,
            },
        );
    }
}

#[test]
fn owned_source_continue_same_type_decoy_has_distinct_owner_lineage() {
    check_with_decoy(true, "arrayOf", true);
    check_with_decoy(false, "arrayOf", true);
}

#[test]
fn owned_source_continue_fresh_inner_owner_structure() {
    let text = include_str!("../../tests/fixtures/owned-source-continue-0312/fresh-inner.ko");
    let (program, origins) = unit_contract::observe(
        text,
        "main",
        &[
            Expected {
                function: "freshLive",
                live: true,
                container: SequentialContainerKind::Array,
                own: false,
            },
            Expected {
                function: "freshDead",
                live: false,
                container: SequentialContainerKind::Array,
                own: false,
            },
        ],
        Control::Continue,
        false,
    );
    assert_eq!(origins.len(), 2);
    for (origin, live) in origins.iter().zip([true, false]) {
        structure::check(&program, live, origin.seed.unwrap());
    }
}

mod array_unit_boundaries {
    include!("lower_frontend_owned_source_continue_tests/array_unit_boundaries.rs");
}

const ARRAY_FIELD_MEMBER_SOURCE: &str = "class Holder(val keys: Array<Int>) { fun scan(): Unit { for (key in this.keys) { if (key == 2) { continue } } } }";

#[test]
fn original_owned_source_return_boundary_uses_unit() {
    unit_contract::observe(
        "fun main(): Unit { val keys = arrayOf(1, 2, 3); for (key in keys) { if (key == 2) { return } }; keys.size }",
        "main",
        &[Expected {
            function: "main",
            live: true,
            container: SequentialContainerKind::Array,
            own: false,
        }],
        Control::Return,
        false,
    );
}

// Original declaration, grouping and alias sources share the full Unit identity contract.
fn assert_continue_identity_unit(text: &str, expected: &[(&str, SequentialContainerKind, usize)]) {
    use crate::{
        native_tests::boxed_enum_tests::unit_source::UnitSource,
        ssa::model::{Operation, Origin},
    };
    use lang_frontend::{
        name_resolution::{SourceUnitKey, SymbolKind},
        ownership_checking::{UnitIterationSourceAccess, UnitLoanTarget},
        type_checking::IterationProvider,
    };

    let unit = UnitSource::analyze("lowering.ko", text).expect("strict original identity source");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    let source = unit.parsed().source_id();
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), source);
    assert_eq!(unit.sources().source_name(source), Ok("lowering.ko"));
    assert_eq!(unit.sources().source_text(source), Ok(text));
    assert_eq!(unit.parsed().roots().len(), expected.len());
    assert_eq!(
        unit.basic_types().types().sequential_iterations().len(),
        expected.len()
    );
    assert_eq!(unit.ownership().iterations().len(), expected.len());
    let main = unit.declaration("main", SymbolKind::Function);
    let (program, entry) = unit
        .lower_entry(main)
        .expect("the unchanged identity source lowers");
    let [module] = program.modules.as_slice() else {
        panic!("one source module");
    };
    assert_eq!(module.functions.len(), expected.len());
    assert_eq!(
        module.function(entry).unwrap().origin,
        Origin::Source(unit.declaration_span(main))
    );
    let mut roots = Vec::new();
    for &(name, container, keys_start) in expected {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let span = unit.declaration_span(declaration);
        let functions = module
            .functions
            .iter()
            .filter(|function| function.origin == Origin::Source(span))
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            panic!("one lowered original declaration {name}");
        };
        let plans = unit
            .ownership()
            .iterations()
            .iter()
            .filter(|plan| {
                let statement = plan.descriptor().statement();
                assert_eq!(statement.source_unit(), document.id());
                let loop_span = unit
                    .parsed()
                    .ast()
                    .statements()
                    .get(statement.statement())
                    .unwrap()
                    .span();
                assert_eq!(loop_span.source_id(), source);
                span.start() <= loop_span.start() && loop_span.end() <= span.end()
            })
            .collect::<Vec<_>>();
        let [plan] = plans.as_slice() else {
            panic!("one actual loop in {name}");
        };
        let descriptor = plan.descriptor();
        assert_eq!(
            unit.types().sequential_iteration(descriptor.statement()),
            Some(descriptor)
        );
        assert_eq!(
            descriptor.provider(),
            IterationProvider::Sequential(container)
        );
        assert_eq!(plan.source_access(), UnitIterationSourceAccess::Owned);
        let Some(UnitLoanTarget::Place(place)) = plan.source() else {
            panic!("the original named source root");
        };
        assert!(place.is_root());
        assert_eq!(place.root().source_unit(), document.id());
        assert!(
            !roots.contains(&place.root()),
            "same-spelled roots in distinct functions cannot collapse"
        );
        roots.push(place.root());
        let symbols = unit.names().names().source_units()[0]
            .resolution()
            .symbols()
            .iter()
            .filter(|symbol| symbol.id() == place.root().symbol())
            .collect::<Vec<_>>();
        let [symbol] = symbols.as_slice() else {
            panic!("one actual keys declaration");
        };
        let keys = unit
            .sources()
            .span(source, keys_start, keys_start + 4)
            .unwrap();
        assert_eq!(unit.sources().slice(keys), Ok("keys"));
        assert_eq!(symbol.span(), keys);
        assert_eq!(symbol.name(), "keys");
        assert!(span.start() <= keys.start() && keys.end() <= span.end());
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|instruction| matches!(
                    instruction.operation,
                    Operation::ContainerElementPlace { .. }
                ))
                .count(),
            1
        );
    }
    let calls = module
        .functions
        .iter()
        .flat_map(|function| &function.instructions)
        .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), expected.len() - 1);
    if expected.len() == 2 {
        let first = unit.declaration("first", SymbolKind::Function);
        let first = module
            .functions
            .iter()
            .find(|function| function.origin == Origin::Source(unit.declaration_span(first)))
            .unwrap();
        let [call] = calls.as_slice() else {
            panic!("one original main-to-first call");
        };
        assert!(
            module
                .function(entry)
                .unwrap()
                .instructions
                .iter()
                .any(|instruction| std::ptr::eq(instruction, *call))
        );
        let Operation::DirectCall {
            callee,
            receiver,
            arguments,
        } = &call.operation
        else {
            unreachable!();
        };
        assert_eq!(*callee, first.id());
        assert!(receiver.is_none() && arguments.is_empty());
        let Origin::Source(span) = call.origin else {
            panic!("original call origin");
        };
        assert_eq!(unit.sources().slice(span), Ok("first()"));
    }
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("original roots and actual call verify through LLVM");
}

#[test]
fn owned_source_continue_declaration_identity_and_groups() {
    for text in [
        "fun main(): Unit { var keys = ((arrayOf(1, 2))); for (key in keys) { if (key == 2) { continue } }; keys.size }",
        "fun first(): Unit { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { continue } }; keys.size }\nfun main(): Unit { val keys = listOf(1, 2); for (key in keys) { if (key == 2) { continue } }; keys.size; first() }",
    ] {
        // Keep the original sources, all reachable roots and verified Unit LLVM.
        let expected = if text.starts_with("fun first") {
            vec![
                ("first", SequentialContainerKind::Array, 24),
                ("main", SequentialContainerKind::List, 131),
            ]
        } else {
            vec![("main", SequentialContainerKind::Array, 23)]
        };
        assert_continue_identity_unit(text, &expected);
    }
    let text = "fun main(): Unit { val original = arrayOf(1, 2); val keys = original; for (key in keys) { if (key == 2) { continue } }; keys.size }";
    assert_continue_identity_unit(text, &[("main", SequentialContainerKind::Array, 53)]);
}

#[test]
fn owned_source_continue_borrowed_element_cannot_be_owned() {
    let text = "fun consume(own text: String): Unit {}\nfun main(): Unit { val keys = arrayOf(\"one\", \"two\"); for (key in keys) { consume(key); if (key == \"two\") { continue } } }";
    let analysis = analyze(text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert_complete_diagnostic(
        text,
        &analysis.sources,
        analysis.owned.diagnostics(),
        ExpectedDiagnostic {
            code: "L0133",
            message: "cannot move a non-Copyable value out of a borrowed binding",
            primary: (120, 123, "key"),
            labels: &[(97, 100, "key", "non-owning binding established here")],
        },
    );
    assert_unit_diagnostic(
        text,
        ExpectedDiagnostic {
            code: "L0133",
            message: "cannot move a non-Copyable value out of a borrowed binding",
            primary: (120, 123, "key"),
            labels: &[
                (97, 100, "key", "non-owning binding established here"),
                (
                    12,
                    28,
                    "own text: String",
                    "selected parameter declared here",
                ),
            ],
        },
    );
}

const ORIGINAL_SHADOW_ARRAYS: &str = "fun main(): Unit { val keys = arrayOf(9); keys.size; block { val keys = arrayOf(1, 2); for (key in keys) { if (key == 2) { continue } }; keys.size } }";
#[path = "lower_frontend_owned_source_continue_tests/array_control_unit.rs"]
mod array_control_unit;

#[path = "lower_frontend_owned_source_continue_tests/external_owner_unit.rs"]
mod external_owner_unit;

const ORIGINAL_LIVE_MAP_SHORT_CIRCUIT: &str = "fun main(): Unit { val keys = arrayOf(1, 2, 3); var first = mutableMapOf<Int, String>(); var second = mutableMapOf<Int, String>(); for (key in keys) { first.put(key, \"first\".clone()); second.put(key, \"second\".clone()); if (key == 2) { continue }; first.put(9, \"tail one\".clone()); second.put(9, \"tail two\".clone()) }; if (first.contains(3) && second.contains(3)) { println(\"maps\") } }";

#[path = "lower_frontend_owned_source_continue_tests/short_circuit_unit.rs"]
mod short_circuit_unit;

#[test]
fn owned_source_continue_external_owner_and_short_circuit_boundaries() {
    // The two former Single-only negative rows are exercised by the exact-source
    // external_owner_unit and short_circuit_unit regressions above.
    let text = "fun main(): Unit { val keys = arrayOf(1, 2, 3); var first = mutableMapOf<Int, String>(); var second = mutableMapOf<Int, String>(); for (key in keys) { first.put(key, \"first\".clone()); second.put(key, \"second\".clone()); if (key == 2) { continue }; println(\"tail\") } }";
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

    let unit = UnitSource::analyze("lowering.ko", text).expect("strict original dead-Map source");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    assert_eq!(unit.parsed().roots().len(), 1);
    let declaration = unit.declaration("main", SymbolKind::Function);
    let (program, entry) = unit
        .lower_entry(declaration)
        .expect("dead Map without post-control use stays supported");
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    assert_eq!(
        function.origin,
        crate::ssa::model::Origin::Source(unit.declaration_span(declaration))
    );
    // Preserve the original no-process-entry renderer contract.
    crate::llvm::render_verified_program(&program).unwrap();
}

#[path = "lower_frontend_owned_source_continue_tests/range_view_unit.rs"]
mod range_view_unit;

fn trusted_range_view_continue_source() -> String {
    format!(
        "{}\nfun main(): Unit {{ val keys = listOf(1, 2); for (key in take(keys, 1)) {{ if (key == 2) {{ continue }} }}; keys.size }}",
        include_str!("../../../lang-std/koven/algorithms/ranges.ko")
            .strip_prefix("package koven.algorithms\n")
            .unwrap()
    )
}
