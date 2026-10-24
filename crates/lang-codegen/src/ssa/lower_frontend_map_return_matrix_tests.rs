//! Frozen ordinary sources shared by SSA and unmodified object/link/run.
use std::collections::BTreeSet;

use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{FunctionId, Operation, Program, TerminatorKind},
};
use lang_frontend::{
    name_resolution::{SymbolKind, UnitReferenceTarget, UnitSymbolId},
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{Expression, Item, NameMarker},
};

fn check(source: &str) -> (UnitSource, Program) {
    let analysis = UnitSource::analyze("lowering.ko", source).unwrap();
    analysis.assert_frontend_clean();
    assert_eq!(
        analysis
            .sources()
            .source_text(analysis.parsed().source_id())
            .unwrap(),
        source,
        "the original fixture is the entire MemoryDocument input"
    );
    let main = analysis.declaration("main", SymbolKind::Function);
    let (program, entry) = analysis.lower_entry(main).unwrap();
    assert_eq!(
        super::structure::source_function(&program, analysis.declaration_span(main))
            .1
            .id,
        entry,
        "the executable entry is the original main, not a synthetic wrapper"
    );
    assert_all_source_functions(&analysis, &program, entry);
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let results = returned_result_symbols(&analysis);
    for fact in analysis.ownership().drops() {
        if let (UnitDropPoint::ControlTransfer(_), UnitDropTarget::Named(symbol)) =
            (fact.point(), fact.target())
        {
            assert!(
                !results.contains(&symbol),
                "the owned return result must be excluded from transfer cleanup"
            );
        }
    }
    for function in program.modules.iter().flat_map(|module| &module.functions) {
        for block in &function.blocks {
            if let TerminatorKind::Return { values } = &block.terminator.as_ref().unwrap().kind {
                for value in values {
                    assert!(
                        !function.instructions.iter().any(|instruction| {
                            matches!(instruction.operation, Operation::Drop { owner } if owner == *value)
                        }),
                        "returned value has a single consuming Return"
                    );
                }
            }
        }
    }
    (analysis, program)
}

// Main-rooted planning must retain every function the old whole-file entry checked.
// Bind each AST root through this analysis's declaration identity to one SSA body.
fn assert_all_source_functions(analysis: &UnitSource, program: &Program, entry: FunctionId) {
    let parsed = analysis.parsed();
    let index = analysis.names().names().index();
    let [source] = index.source_units() else {
        panic!("one original memory document");
    };
    let mut functions = BTreeSet::new();
    for root in parsed.roots() {
        let node = parsed.ast().items().get(*root).unwrap();
        let Item::Function {
            name: NameMarker::Present(name),
            ..
        } = node.payload()
        else {
            continue;
        };
        let declarations = index
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.root() == *root
                    && declaration.source_unit() == source.id()
                    && declaration.kind() == SymbolKind::Function
            })
            .collect::<Vec<_>>();
        let [declaration] = declarations.as_slice() else {
            panic!("one actual declaration for source function root {root:?}");
        };
        assert_eq!(declaration.name_span(), *name);
        assert_eq!(analysis.declaration_span(declaration.id()), node.span());
        let (_, function) = super::structure::source_function(program, node.span());
        assert!(
            functions.insert(function.id),
            "distinct source function roots"
        );
    }
    assert!(functions.contains(&entry));

    // Resource bodies are reached through the real typed deinit descriptor and
    // storage drop registration, not through guessed generated function names.
    let mut deinits = 0;
    for signature in analysis.types().signatures().declarations() {
        let Some(deinit) = signature.nominal().and_then(|nominal| nominal.deinit()) else {
            continue;
        };
        deinits += 1;
        assert_eq!(deinit.owner(), signature.declaration());
        assert_eq!(deinit.item().source_unit(), source.id());
        assert_eq!(deinit.body().source_unit(), source.id());
        let node = parsed.ast().items().get(deinit.item().item()).unwrap();
        assert!(matches!(node.payload(), Item::Deinit { body, .. }
            if *body == deinit.body().statement()));
        let (module, function) = super::structure::source_function(program, node.span());
        assert!(module.deinits.values().any(|id| *id == function.id));
        assert!(
            functions.insert(function.id),
            "distinct source deinit bodies"
        );
    }
    assert_eq!(
        deinits,
        parsed
            .ast()
            .items()
            .iter()
            .filter(|(_, node)| { matches!(node.payload(), Item::Deinit { .. }) })
            .count()
    );
}

// Keep the old all-Named(result) exclusion, but take qualified identities from
// published type facts and bind them to actual declarations and return operands.
fn returned_result_symbols(analysis: &UnitSource) -> BTreeSet<UnitSymbolId> {
    let parsed = analysis.parsed();
    let names = analysis.names().names();
    let [source] = names.source_units() else {
        panic!("one original source's names");
    };
    let return_names = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            let Expression::Return {
                value: Some(value), ..
            } = node.payload()
            else {
                return None;
            };
            let mut value = *value;
            loop {
                let operand = parsed.ast().expressions().get(value).unwrap();
                match operand.payload() {
                    Expression::Group { expression } => value = *expression,
                    Expression::Name => return Some(operand.span()),
                    _ => return None,
                }
            }
        })
        .collect::<Vec<_>>();
    let mut results = BTreeSet::new();
    for symbol in source
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == "result")
    {
        assert_eq!(symbol.kind(), SymbolKind::Variable);
        assert_eq!(symbol.span().source_id(), parsed.source_id());
        assert_eq!(
            parsed
                .ast()
                .items()
                .iter()
                .filter(|(_, node)| {
                    matches!(node.payload(), Item::Variable { name: NameMarker::Present(span), .. }
                if *span == symbol.span())
                })
                .count(),
            1,
            "one real result binding declaration"
        );
        let qualified = analysis
            .types()
            .body_symbol_types()
            .keys()
            .copied()
            .filter(|id| id.source_unit() == source.source_unit() && id.symbol() == symbol.id())
            .collect::<Vec<_>>();
        let [qualified] = qualified.as_slice() else {
            panic!("one published qualified identity for the actual result binding");
        };
        assert!(names.references().iter().any(|reference| {
            reference.source_unit() == source.source_unit()
                && return_names.contains(&reference.span())
                && matches!(reference.target(), UnitReferenceTarget::Symbol(id) if id == qualified)
        }), "the result identity is the actual direct or grouped Return operand");
        assert!(results.insert(*qualified));
    }
    results
}

#[test]
fn single_map_return_cardinality_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/live-dead.ko"
    ));
}
#[test]
fn single_map_return_distinct_owner_matrix() {
    let (analysis, program) = check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/distinct-owners.ko"
    ));
    super::structure::assert_distinct_return_owners(
        &program,
        analysis.declaration_span(analysis.declaration("separateOwners", SymbolKind::Function)),
    );
}
#[test]
fn single_map_return_provider_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/borrowed-temporary-providers.ko"
    ));
}
#[test]
fn single_map_return_operand_cleanup_matrix() {
    let source =
        include_str!("../../tests/fixtures/map-conditional-return-0313/operand-cleanup.ko");
    let (analysis, program) = check(source);
    super::structure::assert_operand_before_body_cleanup(
        &program,
        source,
        analysis.declaration_span(analysis.declaration("cleanup", SymbolKind::Function)),
        analysis.declaration_span(analysis.declaration("operand", SymbolKind::Function)),
    );
}

#[test]
fn single_map_return_owned_results_matrix() {
    check(include_str!(
        "../../tests/fixtures/map-conditional-return-0313/owned-results.ko"
    ));
}

#[test]
fn owned_source_return_operand_cleanup_structure() {
    let source = include_str!("../../tests/fixtures/owned-source-return-0314/operand-cleanup.ko");
    let (analysis, program) = check(source);
    super::structure::assert_owned_operand_before_body_cleanup(
        &program,
        source,
        analysis.declaration_span(analysis.declaration("cleanup", SymbolKind::Function)),
        analysis.declaration_span(analysis.declaration("operand", SymbolKind::Function)),
    );
}

#[test]
fn owned_source_return_distinct_maps_structure() {
    let (analysis, program) = check(include_str!(
        "../../tests/fixtures/owned-source-return-0314/distinct-owners.ko"
    ));
    super::structure::assert_distinct_return_owners(
        &program,
        analysis.declaration_span(analysis.declaration("separateOwners", SymbolKind::Function)),
    );
}

#[test]
fn owned_source_return_independent_results_structure() {
    check(include_str!(
        "../../tests/fixtures/owned-source-return-0314/owned-results.ko"
    ));
}
