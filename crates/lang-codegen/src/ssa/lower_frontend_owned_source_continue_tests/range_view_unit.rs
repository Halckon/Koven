//! Ordinary Unit accepts the original trusted View, not Sequential authorization.
use super::trusted_range_view_continue_source;
use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{Block, Edge, EntityId, Function, LoanKind, Operation, TerminatorKind},
};
use lang_frontend::{
    name_resolution::{SourceUnitKey, SymbolKind},
    ownership_checking::{
        RangeUseSite, UnitIterationCleanupAction as Action, UnitIterationExitKind, UnitLoanTarget,
    },
    type_checking::IterationProvider,
};
use std::collections::BTreeSet;

#[test]
fn trusted_range_view_continue_preserves_unit_provider_and_cleanup() {
    let text = trusted_range_view_continue_source();
    let unit =
        UnitSource::analyze_with_type_environment("trusted.ko", &text, |map, source, env| {
            env.authorize_range_source(map, source).unwrap();
            env.authorize_range_extension_source(map, source).unwrap();
        })
        .unwrap();
    unit.assert_frontend_clean();
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original source");
    };
    let source_id = document.source_id();
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source_id, unit.parsed().source_id());
    assert_eq!(unit.sources().source_name(source_id), Ok("trusted.ko"));
    assert_eq!(unit.sources().source_text(source_id), Ok(text.as_str()));

    let [typed] = unit.types().sequential_iterations() else {
        panic!("one original provider");
    };
    let [owned] = unit.ownership().iterations() else {
        panic!("one original ownership plan");
    };
    assert_eq!(typed.provider(), IterationProvider::RangeView);
    assert_eq!(owned.descriptor(), typed);
    let expr_span = |id| unit.parsed().ast().expressions().get(id).unwrap().span();
    let take_span = expr_span(typed.source().expression());
    assert_eq!(unit.sources().slice(take_span), Ok("take(keys, 1)"));
    let short = unit
        .ownership()
        .borrow_results()
        .range_uses()
        .iter()
        .filter(|fact| fact.site() == RangeUseSite::Iteration)
        .collect::<Vec<_>>();
    let [range] = short.as_slice() else {
        panic!("one checked short-lived View");
    };
    assert_eq!(range.expression(), typed.source());
    assert_eq!(owned.source(), Some(range.origin()));
    let UnitLoanTarget::Place(place) = range.origin() else {
        panic!("View borrows the existing keys owner");
    };
    assert!(place.is_root());
    let root = place.root();
    let checked = &unit.names().names().source_units()[root.source_unit().index()];
    let symbol = checked
        .resolution()
        .symbols()
        .get(root.symbol().index())
        .unwrap();
    assert_eq!(symbol.name(), "keys");

    let continue_exit = owned
        .exits()
        .iter()
        .find_map(|exit| {
            let UnitIterationExitKind::Continue(id) = exit.kind() else {
                return None;
            };
            Some((exit, expr_span(id.expression())))
        })
        .unwrap();
    assert_eq!(unit.sources().slice(continue_exit.1), Ok("continue"));
    let statement = typed.statement();
    assert_eq!(
        unit.ownership()
            .iteration_cleanup_at(continue_exit.0.point()),
        Some(continue_exit.0.actions())
    );
    assert!(matches!(
        continue_exit.0.actions(),
        [Action::EndBinding { statement: binding, .. }, Action::EndElement(element)]
            if *binding == statement && *element == statement
    ));
    let exhausted = owned
        .exits()
        .iter()
        .find(|exit| exit.kind() == UnitIterationExitKind::Exhaustion)
        .unwrap();
    assert_eq!(
        exhausted.actions(),
        &[
            Action::FinishProvider(statement),
            Action::EndSource(statement)
        ]
    );

    let declaration = unit.declaration("main", SymbolKind::Function);
    let (lowered, routes) = crate::ssa::refine::observe_routing(|| unit.lower_entry(declaration));
    let (program, entry_id) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
    let module = program.module(entry_id.module()).unwrap();
    let entry = module.function(entry_id).unwrap();
    assert!(routes.contains(&entry.name));
    assert_eq!(entry.origin.span(), unit.declaration_span(declaration));

    let take_calls = entry
        .instructions
        .iter()
        .filter(|i| {
            i.origin.span() == take_span && matches!(i.operation, Operation::RangeCall { .. })
        })
        .collect::<Vec<_>>();
    let [take] = take_calls.as_slice() else {
        panic!("unique exact source RangeCall");
    };
    let Operation::RangeCall { callee, source, .. } = take.operation else {
        unreachable!()
    };
    let method = module.function(callee).unwrap();
    assert!(
        unit.sources()
            .slice(method.origin.span())
            .unwrap()
            .contains("fun <T> take")
    );
    let [EntityId::Value(view), EntityId::Loan(protecting)] = take.results.as_slice() else {
        panic!("RangeCall returns the descriptor and its protecting loan");
    };
    assert_ne!(*protecting, source);
    let [keys] = entry
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation, Operation::ContainerConstruct { .. })
                && unit.sources().slice(i.origin.span()) == Ok("listOf(1, 2)")
        })
        .collect::<Vec<_>>()[..]
    else {
        panic!("one source-matched keys construction");
    };
    let [owner @ EntityId::Value(_)] = keys.results.as_slice() else {
        panic!("keys owner");
    };
    let pairs = |value, loan| (carries(entry, value), carries(entry, EntityId::Loan(loan)));
    let (owner_ids, input_ids) = pairs(*owner, source);
    let (view_ids, protecting_ids) = pairs(EntityId::Value(*view), *protecting);
    let root_places = |owners: &BTreeSet<EntityId>| -> BTreeSet<EntityId> {
        entry.instructions.iter().filter_map(|i| {
            matches!(i.operation, Operation::RootPlace { owner } if owners.contains(&EntityId::Value(owner)))
                .then(|| i.results.first().copied()).flatten()
        }).flat_map(|id| carries(entry, id)).collect()
    };
    let keys_places = root_places(&owner_ids);
    let view_places = root_places(&view_ids);
    let borrowed = |places: &BTreeSet<EntityId>| -> BTreeSet<EntityId> {
        entry
            .instructions
            .iter()
            .filter_map(|i| {
                matches!(i.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared }
                if places.contains(&EntityId::Place(place)))
                .then(|| i.results.first().copied())
                .flatten()
            })
            .flat_map(|id| carries(entry, id))
            .collect()
    };
    assert!(borrowed(&keys_places).contains(&EntityId::Loan(source)));
    let metadata = borrowed(&view_places);
    let elements = entry
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation, Operation::RangeElementPlace { view, .. }
            if metadata.contains(&EntityId::Loan(view)))
        })
        .collect::<Vec<_>>();
    let [element] = elements.as_slice() else {
        panic!("one actual RangeView element");
    };
    assert!(
        !entry
            .instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::ContainerElementPlace { .. }))
    );
    let element_loan = borrowed(&carries(entry, element.results[0]));

    let continuing = entry
        .blocks
        .iter()
        .filter(|b| {
            b.terminator
                .as_ref()
                .is_some_and(|t| t.origin.span() == continue_exit.1)
        })
        .collect::<Vec<_>>();
    let [continuing] = continuing.as_slice() else {
        panic!("one actual Continue edge");
    };
    let TerminatorKind::Branch(edge) = &continuing.terminator.as_ref().unwrap().kind else {
        panic!("Continue branches to the header");
    };
    let header = entry.block(edge.target).unwrap();
    let TerminatorKind::Conditional { when_false, .. } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("Continue reaches the provider availability test");
    };
    let exhausted = entry.block(when_false.target).unwrap();
    let continue_operations = operations(entry, continuing);
    assert_eq!(continue_operations.iter().filter(|op| {
        matches!(op, Operation::BorrowEnd { loan } if element_loan.contains(&EntityId::Loan(*loan)))
    }).count(), 1);
    assert!(!continue_operations.iter().any(|op| match op {
        Operation::RangeEnd { .. } => true,
        Operation::BorrowEnd { loan } =>
            metadata.contains(&EntityId::Loan(*loan)) || input_ids.contains(&EntityId::Loan(*loan)),
        Operation::Drop { owner } => owner_ids.contains(&EntityId::Value(*owner)),
        _ => false,
    }));

    let exhaustion_operations = operations(entry, exhausted);
    let unique_position = |predicate: &dyn Fn(&Operation) -> bool| {
        let positions = exhaustion_operations
            .iter()
            .enumerate()
            .filter_map(|(index, op)| predicate(op).then_some(index))
            .collect::<Vec<_>>();
        let [position] = positions.as_slice() else {
            panic!("one matching actual cleanup operation");
        };
        *position
    };
    let metadata_end = unique_position(
        &|op| matches!(op, Operation::BorrowEnd { loan } if metadata.contains(&EntityId::Loan(*loan))),
    );
    let range_end = unique_position(
        &|op| matches!(op, Operation::RangeEnd { view, source } if view_ids.contains(&EntityId::Value(*view)) && protecting_ids.contains(&EntityId::Loan(*source))),
    );
    let source_end = unique_position(
        &|op| matches!(op, Operation::BorrowEnd { loan } if input_ids.contains(&EntityId::Loan(*loan))),
    );
    assert!(metadata_end < range_end && range_end < source_end);
}

fn operations<'a>(function: &'a Function, block: &Block) -> Vec<&'a Operation> {
    block
        .instructions
        .iter()
        .map(|id| &function.instruction(*id).unwrap().operation)
        .collect()
}

/// Block parameters receive the exact entity at the corresponding real edge operand.
fn carries(function: &Function, original: EntityId) -> BTreeSet<EntityId> {
    let mut known = BTreeSet::from([original]);
    loop {
        let previous = known.len();
        for block in &function.blocks {
            let mut propagate = |edge: &Edge| {
                for (&from, &to) in edge
                    .arguments
                    .iter()
                    .zip(&function.block(edge.target).unwrap().parameters)
                {
                    if known.contains(&from) {
                        known.insert(to);
                    }
                }
            };
            match block.terminator.as_ref().map(|t| &t.kind) {
                Some(TerminatorKind::Branch(edge)) => propagate(edge),
                Some(TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                }) => {
                    propagate(when_true);
                    propagate(when_false);
                }
                _ => {}
            }
        }
        if known.len() == previous {
            return known;
        }
    }
}
