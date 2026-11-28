const RESOURCE_SOURCE: &str = "class Item(val id: Int) { deinit() {} }\nfun main(): Unit { val keys = arrayOf(Item(1), Item(2)); for (key in keys) { if (key.id == 2) { continue } }; keys.size }";
const INOUT: &str = "fun scan(inout keys: Array<Int>): Unit { for (key in keys) { if (key == 2) { continue } }; keys.size }";
const REPLACEMENT_SEED: &str = "fun main(): Unit { var keys = arrayOf(1, 2); keys = arrayOf(2, 3); for (key in keys) { if (key == 2) { continue } }; keys.size }";

use super::{SequentialContainerKind, structure};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::ssa::model::{EntityId, Operation};
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::{
        UnitIterationCleanupAction as Action, UnitIterationExitKind, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    type_checking::IterationProvider,
};

fn provider(unit: &UnitSource, access: UnitIterationSourceAccess) {
    unit.assert_frontend_clean();
    let [typed] = unit.types().sequential_iterations() else {
        panic!("one typed provider");
    };
    let [plan] = unit.ownership().iterations() else {
        panic!("one ownership plan");
    };
    assert_eq!(plan.descriptor(), typed);
    assert_eq!(
        typed.provider(),
        IterationProvider::Sequential(SequentialContainerKind::Array)
    );
    let source = unit
        .parsed()
        .ast()
        .expressions()
        .get(typed.source().expression())
        .unwrap()
        .span();
    assert_eq!(unit.sources().slice(source), Ok("keys"));
    assert_eq!(plan.source_access(), access);
    let Some(UnitLoanTarget::Place(place)) = plan.source() else {
        panic!("named array");
    };
    assert!(place.is_root());
    let root = place.root();
    let source = &unit.names().names().source_units()[root.source_unit().index()];
    assert_eq!(
        source
            .resolution()
            .symbols()
            .get(root.symbol().index())
            .unwrap()
            .name(),
        "keys"
    );
    let continuing = plan
        .exits()
        .iter()
        .find(|edge| matches!(edge.kind(), UnitIterationExitKind::Continue(_)))
        .unwrap();
    let statement = typed.statement();
    assert!(matches!(continuing.actions(),
        [Action::EndBinding { statement: bound, .. }, Action::EndElement(element)]
        if *bound == statement && *element == statement));
    assert_eq!(
        unit.ownership().iteration_cleanup_at(continuing.point()),
        Some(continuing.actions())
    );
}

#[test]
fn original_inout_array_continue_stops_at_unit_callable_boundary() {
    let unit = UnitSource::analyze("lowering.ko", INOUT).unwrap();
    provider(&unit, UnitIterationSourceAccess::Exclusive);
    assert!(unit.ownership().drops().is_empty());
    let error = unit
        .lower_entry(unit.declaration("scan", SymbolKind::Function))
        .err()
        .expect("valid provider does not enable the inout-array ABI");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
    let source = unit.parsed().source_id();
    let span = error.span.unwrap();
    assert_eq!(span, unit.sources().span(source, 9, 31).unwrap());
    assert_eq!(unit.sources().source_text(source), Ok(INOUT));
    assert_eq!(unit.sources().slice(span), Ok("inout keys: Array<Int>"));
}

#[test]
fn original_resource_and_replacement_arrays_keep_unit_continue_owner() {
    for (text, seed, previous) in [
        (RESOURCE_SOURCE, "arrayOf(Item(1), Item(2))", None),
        (REPLACEMENT_SEED, "arrayOf(2, 3)", Some("arrayOf(1, 2)")),
    ] {
        let unit = UnitSource::analyze("lowering.ko", text).unwrap();
        provider(&unit, UnitIterationSourceAccess::Owned);
        let source = unit.parsed().source_id();
        assert_eq!(unit.sources().source_text(source), Ok(text));
        let at = |fragment: &str| {
            let start = text.find(fragment).unwrap();
            unit.sources()
                .span(source, start, start + fragment.len())
                .unwrap()
        };
        let (program, id) = unit
            .lower_entry(unit.declaration("main", SymbolKind::Function))
            .unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, id).unwrap();
        structure::check(&program, true, at(seed));
        let module = program.module(id.module()).unwrap();
        let main = module.function(id).unwrap();
        let seed = main
            .instructions
            .iter()
            .find(|i| {
                i.origin.span() == at(seed)
                    && matches!(i.operation, Operation::ContainerConstruct { .. })
            })
            .unwrap();
        if let Some(previous) = previous {
            let initial = main
                .instructions
                .iter()
                .find(|i| {
                    i.origin.span() == at(previous)
                        && matches!(i.operation, Operation::ContainerConstruct { .. })
                })
                .unwrap();
            let [EntityId::Value(old)] = initial.results.as_slice() else {
                panic!("old array value");
            };
            let mut ends = main
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::Drop { owner } if owner == *old));
            let end = ends
                .next()
                .expect("old array drops before using the new seed");
            assert!(ends.next().is_none());
            assert_eq!(initial.block, seed.block);
            assert_eq!(end.block, seed.block);
            let block = main.block(seed.block).unwrap();
            let position = |id| block.instructions.iter().position(|i| *i == id).unwrap();
            assert!(
                position(initial.id) < position(end.id) && position(end.id) < position(seed.id)
            );
        } else {
            let Operation::ContainerConstruct { elements, .. } = &seed.operation else {
                unreachable!()
            };
            let [first, second] = elements.as_slice() else {
                panic!("two genuine resource elements");
            };
            assert_ne!(first, second);
            for owner in [*first, *second] {
                let ty = main
                    .entity(EntityId::Value(owner))
                    .unwrap()
                    .ty
                    .semantic_type();
                let deinit = module
                    .deinits
                    .get(&ty)
                    .and_then(|id| module.function(*id))
                    .unwrap();
                assert_eq!(
                    unit.sources().slice(deinit.origin.span()),
                    Ok("deinit() {}")
                );
            }
        }
    }
}

#[test]
fn original_member_field_array_has_unit_provider_but_no_entry_declaration() {
    use lang_frontend::{name_resolution::SourceUnitKey, type_checking::UnitCallableTarget};
    let text = super::ARRAY_FIELD_MEMBER_SOURCE;
    let unit = UnitSource::analyze("lowering.ko", text).unwrap();
    unit.assert_frontend_clean();
    let index = unit.names().names().index();
    let source_id = unit.parsed().source_id();
    let [document] = index.source_units() else {
        panic!("one actual source");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), source_id);
    assert_eq!(unit.sources().source_name(source_id), Ok("lowering.ko"));
    assert_eq!(unit.sources().source_text(source_id), Ok(text));

    let [holder] = index.declarations() else {
        panic!("only the classifier is indexed as a declaration");
    };
    assert_eq!(holder.name(), "Holder");
    assert_eq!(holder.kind(), SymbolKind::Classifier);
    let signature = unit.types().signatures().declaration(holder.id()).unwrap();
    assert!(signature.callable().is_none());
    let [member] = signature.nominal().unwrap().members() else {
        panic!("actual lone instance member");
    };
    assert_eq!(member.name(), "scan");
    assert!(member.receiver().is_some());
    assert_eq!(unit.sources().slice(member.name_span()), Ok("scan"));
    let UnitCallableTarget::Symbol(method) = member.target() else {
        panic!("member must not become a declaration entry");
    };
    assert_eq!(method.source_unit(), document.id());
    let names = &unit.names().names().source_units()[method.source_unit().index()];
    let symbol = names
        .resolution()
        .symbols()
        .get(method.symbol().index())
        .unwrap();
    assert_eq!(symbol.name(), "scan");

    let [typed] = unit.types().sequential_iterations() else {
        panic!("same provider");
    };
    let [plan] = unit.ownership().iterations() else {
        panic!("same ownership plan");
    };
    assert_eq!(
        typed.provider(),
        IterationProvider::Sequential(SequentialContainerKind::Array)
    );
    assert_eq!(plan.descriptor(), typed);
    assert_eq!(plan.source_access(), UnitIterationSourceAccess::Shared);
    let expression_span = |id| unit.parsed().ast().expressions().get(id).unwrap().span();
    let span = expression_span(typed.source().expression());
    assert_eq!(span, unit.sources().span(source_id, 68, 77).unwrap());
    assert_eq!(unit.sources().slice(span), Ok("this.keys"));
    let Some(UnitLoanTarget::Place(place)) = plan.source() else {
        panic!("borrow real keys field");
    };
    assert!(
        place.is_root(),
        "named field represented by its own Unit symbol"
    );
    let root = place.root();
    assert_eq!(root.source_unit(), document.id());
    assert_eq!(
        names
            .resolution()
            .symbols()
            .get(root.symbol().index())
            .unwrap()
            .name(),
        "keys"
    );
    let continue_exit = plan
        .exits()
        .iter()
        .find_map(|exit| {
            let UnitIterationExitKind::Continue(id) = exit.kind() else {
                return None;
            };
            Some((exit, id))
        })
        .unwrap();
    assert_eq!(continue_exit.1.source_unit(), document.id());
    assert_eq!(
        unit.sources()
            .slice(expression_span(continue_exit.1.expression())),
        Ok("continue")
    );
    let statement = typed.statement();
    assert!(matches!(continue_exit.0.actions(),
        [Action::EndBinding { statement: bound, .. }, Action::EndElement(element)]
        if *bound == statement && *element == statement));
    assert_eq!(
        unit.ownership()
            .iteration_cleanup_at(continue_exit.0.point()),
        Some(continue_exit.0.actions())
    );
    assert!(
        unit.ownership().drops().is_empty(),
        "receiver field belongs to the caller"
    );

    // The ordinary entry planner requires a top-level DeclarationId. This exact
    // source publishes only Holder's Symbol-target method, so there is no Unit
    // backend invocation or SSA claim, and no new synthetic top-level caller.
}
