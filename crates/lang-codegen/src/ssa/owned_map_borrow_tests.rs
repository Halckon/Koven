//! SPEC-0299: ordinary-source non-consuming Map result Borrow, without IR mutation.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, resolve_names},
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

fn check(payload: &str, grouped: bool, compilation_unit: bool) {
    let source = crate::native_owned_map_borrow_cases::core(payload, grouped);
    let program = lower(&source, compilation_unit);
    assert_core(&program);
}

pub(super) fn lower(source: &str, compilation_unit: bool) -> super::model::Program {
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let result = if compilation_unit {
        let (provider, entry) = source.split_once("fun entry()").unwrap();
        let (p, provider) = super::unit_lower_test_support::parsed(
            &mut sources,
            "provider.ko",
            &format!("package p\n{provider}"),
        );
        let (q, entry) = super::unit_lower_test_support::parsed(
            &mut sources,
            "entry.ko",
            &format!("package q\nimport p.*\nfun entry(){entry}"),
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", p, &provider),
            SourceUnitInput::new("root", "q/entry.ko", q, &entry),
        ];
        let (names, typed, owned) =
            super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        super::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            super::unit_lower_test_support::declaration(&names, "q", "entry"),
        )
        .map(|(program, _)| program)
    } else {
        let id = sources.add_source("result.ko", source.to_owned()).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        assert!(names.diagnostics().is_empty());
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        super::lower_frontend::orchestrate::lower_scalar_file(
            &sources, &parsed, &names, &typed, &owned,
        )
    };
    let program = result.unwrap_or_else(|error| {
        let origin = error.span.map(|span| {
            (
                sources.source_name(span.source_id()).unwrap(),
                sources.slice(span).unwrap(),
            )
        });
        panic!("unit={compilation_unit}: {error:?}; origin={origin:?}")
    });
    super::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    program
}

macro_rules! case {
    ($name:ident, $payload:literal, $grouped:literal, $unit:literal) => {
        #[test]
        fn $name() {
            check($payload, $grouped, $unit);
        }
    };
}
case!(owned_map_borrow_single_string_plain, "String", false, false);
case!(owned_map_borrow_single_string_group, "String", true, false);
case!(owned_map_borrow_unit_string_plain, "String", false, true);
case!(owned_map_borrow_unit_string_group, "String", true, true);
case!(owned_map_borrow_single_inline_plain, "Text", false, false);
case!(owned_map_borrow_single_inline_group, "Text", true, false);
case!(owned_map_borrow_unit_inline_plain, "Text", false, true);
case!(owned_map_borrow_unit_inline_group, "Text", true, true);
case!(
    owned_map_borrow_single_resource_plain,
    "Resource",
    false,
    false
);
case!(
    owned_map_borrow_single_resource_group,
    "Resource",
    true,
    false
);
case!(
    owned_map_borrow_unit_resource_plain,
    "Resource",
    false,
    true
);
case!(owned_map_borrow_unit_resource_group, "Resource", true, true);

use super::model::*;
fn entry(program: &Program) -> &Function {
    program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name.contains("entry"))
        .expect("ordinary entry function")
}
fn assert_core(program: &Program) {
    let entry = entry(program);
    let calls: Vec<_> = entry
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .collect();
    assert!(calls.len() >= 2);
    for call in &calls[..2] {
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let [EntityId::Loan(loan)] = arguments.as_slice() else {
            panic!("non-consuming loan argument: {arguments:?}")
        };
        assert!(matches!(
            entry.entity(EntityId::Loan(*loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ));
        assert!(entry.blocks.iter().any(|block| matches!(block.terminator.as_ref().map(|term| &term.kind), Some(TerminatorKind::NullableBranch { view, .. }) if view == loan)), "caller loan comes from nullable proof, not copied payload");
    }
    let first = calls[0].id.index();
    let second = calls[1].id.index();
    let takes: Vec<_> = entry
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::NullableTake { .. }))
        .collect();
    assert_eq!(
        takes.len(),
        1,
        "only explicit post-Borrow !! consumes owner"
    );
    assert!(takes[0].id.index() > second);
    assert!(
        !entry.instructions[first..=second]
            .iter()
            .any(|instruction| matches!(
                instruction.operation,
                Operation::Copy { .. }
                    | Operation::StringClone { .. }
                    | Operation::MapResultUnwrap { .. }
                    | Operation::NullableTake { .. }
            )),
        "Borrow bridge does not copy/clone/take"
    );
}

#[test]
fn owned_map_borrow_scope_prefix_and_last_use_ssa() {
    use crate::native_owned_map_borrow_cases as cases;
    for unit in [false, true] {
        for source in [
            cases::PAIR,
            cases::SCOPES,
            cases::MISSING,
            cases::RESOURCE_SCOPE,
            cases::ABORT,
        ] {
            lower(source, unit);
        }
        for payload in ["String", "Text", "Resource"] {
            let program = lower(&cases::last_use(payload), unit);
            assert!(
                !entry(&program)
                    .instructions
                    .iter()
                    .any(|instruction| matches!(
                        instruction.operation,
                        Operation::NullableTake { .. }
                    ))
            );
        }
        lower(&cases::inline_field(unit), unit);
        lower(&cases::inline_explicit_clone(), unit);
    }
}
