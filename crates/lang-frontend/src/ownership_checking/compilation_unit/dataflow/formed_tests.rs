//! CI-only assignment rollback guard; inspect raw history before aggregate clearing.
mod publication;

use super::super::formed_tests::{Fixture, Parts};
use super::*;
use crate::ownership_checking::{
    UnitFieldReplaceOwnershipPlan, UnitOwnershipPrimitiveOwnershipPlan,
};
use crate::parser::Expression;

const SOURCE: &str = r#"
fun inspect(value: String): Int = 1
class Holder(var target: Int) {
    inout fun update(flag: Boolean): Unit {
        val prefix = "kept"
        val prior = inspect(prefix)
        target = if (flag) { val rhs = "new"; inspect(rhs) } else { 2 }
    }
}
"#;

type FactLengths = (
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
);

#[derive(Debug, PartialEq, Eq)]
struct Facts {
    loans: Vec<UnitLoanFact>,
    deliveries: Vec<UnitValueDeliveryFact>,
    receivers: Vec<UnitReceiverOwnershipFact>,
    rc: Vec<UnitRcOwnershipEffect>,
    constructions: Vec<UnitConstructionOwnershipPlan>,
    assertions: Vec<UnitNonNullAssertionOwnershipPlan>,
    primitives: Vec<UnitOwnershipPrimitiveOwnershipPlan>,
    replacements: Vec<UnitFieldReplaceOwnershipPlan>,
    formed: Vec<UnitSymbolId>,
}

impl Facts {
    fn read(checker: &Checker<'_>) -> Self {
        Self {
            loans: checker.loans.clone(),
            deliveries: checker.value_deliveries.clone(),
            receivers: checker.receiver_facts.clone(),
            rc: checker.rc_effects.clone(),
            constructions: checker.construction_plans.clone(),
            assertions: checker.non_null_assertions.clone(),
            primitives: checker.ownership_primitives.clone(),
            replacements: checker.field_replacements.clone(),
            formed: checker.formed_owned_locals.clone(),
        }
    }

    fn lengths(&self) -> FactLengths {
        (
            self.loans.len(),
            self.deliveries.len(),
            self.receivers.len(),
            self.rc.len(),
            self.constructions.len(),
            self.assertions.len(),
            self.primitives.len(),
            self.replacements.len(),
            self.formed.len(),
        )
    }
}

// Reconstruct exactly the production dataflow input chain; only field_kinds may differ.
fn with_checker<T>(
    fixture: &Fixture,
    parts: &Parts,
    immutable: bool,
    inspect: impl FnOnce(&mut Checker<'_>) -> T,
) -> T {
    let inputs = fixture.inputs();
    let mut fields = traversal::collect_field_mutability(&inputs, fixture.names.names()).unwrap();
    let target = fixture.symbol(0, "target");
    assert_eq!(fields.get(&target), Some(&VariableKind::Var));
    if immutable {
        let original = fields.clone();
        fields.insert(target, VariableKind::Val);
        assert_eq!(fields.len(), original.len());
        assert_eq!(
            fields
                .iter()
                .filter(|(symbol, kind)| original.get(*symbol) != Some(*kind))
                .count(),
            1
        );
    }
    let constructions = collect_construction_descriptors(
        fixture.typed.types().constructions(),
        fixture.names.names().index().source_units().len(),
    )
    .unwrap();
    let empty = BTreeMap::new();
    let catalog = codes::catalog().unwrap();
    let codes = Codes {
        use_after_move: catalog.resolve(codes::USE_AFTER_MOVE).unwrap(),
        partial_move: catalog.resolve(codes::PARTIAL_MOVE).unwrap(),
        borrowed_move: catalog.resolve(codes::MOVE_FROM_BORROWED_BINDING).unwrap(),
        loan_conflict: catalog.resolve(codes::LOAN_CONFLICT).unwrap(),
        immutable_inout: catalog.resolve(codes::IMMUTABLE_INOUT_PLACE).unwrap(),
        container_element_move: catalog.resolve(codes::MOVE_FROM_CONTAINER_ELEMENT).unwrap(),
        borrowed_closure_escape: catalog.resolve(codes::BORROWED_CLOSURE_ESCAPE).unwrap(),
        illegal_owned_capture: catalog.resolve(codes::ILLEGAL_OWNED_CAPTURE).unwrap(),
        non_transferable_delivery: catalog.resolve(codes::NON_TRANSFERABLE_DELIVERY).unwrap(),
    };
    let mut diagnostics = Vec::new();
    let mut loans = Vec::new();
    let mut deliveries = Vec::new();
    let mut receivers = Vec::new();
    let mut conditional = Vec::new();
    let mut rc = Vec::new();
    let mut construction_plans = Vec::new();
    let mut assertions = Vec::new();
    let mut callable_sources = callable_provenance::Collection::default();
    let mut checker = Checker::new(
        &fixture.sources,
        &fixture.parsed[0],
        fixture.source(0),
        fixture.names.names(),
        fixture.typed.types(),
        &parts.bindings,
        &fields,
        &parts.arguments,
        &parts.receivers,
        &parts.capture.captures,
        &parts.capture.closures,
        &parts.capture.transferabilities,
        constructions.get(&fixture.source(0)).unwrap_or(&empty),
        codes,
        &mut diagnostics,
        &mut loans,
        &mut deliveries,
        &mut receivers,
        &mut conditional,
        &mut rc,
        &mut construction_plans,
        &mut assertions,
        &mut callable_sources,
    )
    .unwrap();
    checker.range_producers = crate::ownership_checking::range_producer::collect_unit(
        &fixture.sources,
        &inputs,
        fixture.names.names(),
        fixture.typed.types(),
    )
    .unwrap();
    let drop_analysis = checker.run().unwrap();
    assert!(drop_analysis.deferred.is_empty());
    assert!(checker.primitive_deferred.is_empty());
    inspect(&mut checker)
}

#[test]
fn rejected_assignment_restores_only_new_formed_history() {
    let fixture = Fixture::new(&[("assignment.ko", SOURCE)]);
    let shared = fixture.shared();
    assert!(
        shared.diagnostics().is_empty(),
        "{:?}",
        shared.diagnostics()
    );
    assert!(shared.deferred().is_empty(), "{:?}", shared.deferred());
    shared.clone().validate().unwrap();
    let parts = fixture.parts(&shared);
    assert_eq!(
        parts.arguments.len(),
        2,
        "real prefix and RHS call contracts"
    );
    assert!(!parts.bindings.is_empty());
    let prefix = fixture.local(0, "prefix");
    let prior = fixture.local(0, "prior");
    let rhs = fixture.local(0, "rhs");
    let mut published = vec![prefix, prior, rhs];
    published.sort();
    assert_eq!(shared.formed_owned_locals(), published);

    // A successful real RHS must actually append rhs; otherwise no negative claim is valid.
    let baseline = with_checker(&fixture, &parts, false, |checker| {
        assert!(checker.diagnostics.is_empty());
        assert_eq!(checker.formed_owned_locals, [prefix, prior, rhs]);
        Facts::read(checker)
    });
    assert_eq!(
        baseline.loans.len(),
        2,
        "prefix and RHS both establish real loans"
    );
    let assignment = fixture.parsed[0]
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Expression::Assignment { target, value, .. } => Some((*target, *value)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(assignment.len(), 1);
    let (target, value) = assignment[0];
    let target_span = fixture.parsed[0]
        .ast()
        .expressions()
        .get(target)
        .unwrap()
        .span();
    assert_eq!(fixture.sources.slice(target_span).unwrap(), "target");
    let value_span = fixture.parsed[0]
        .ast()
        .expressions()
        .get(value)
        .unwrap()
        .span();
    let rhs_span = fixture.names.names().source_units()[rhs.source_unit().index()]
        .resolution()
        .symbols()[rhs.symbol().index()]
    .span();
    assert_eq!(rhs_span.source_id(), value_span.source_id());
    assert!(value_span.start() < rhs_span.start() && rhs_span.end() < value_span.end());

    let rejected = with_checker(&fixture, &parts, true, |checker| {
        assert_eq!(checker.diagnostics.len(), 1);
        let diagnostic = &checker.diagnostics[0];
        assert_eq!(diagnostic.code().to_string(), "L0134");
        assert_eq!(diagnostic.primary_span(), target_span);
        assert_eq!(
            diagnostic.message(),
            "assignment target is not a mutable place"
        );
        let target_symbol = fixture.symbol(0, "target");
        let mut expected = Diagnostic::new(
            &fixture.sources,
            Severity::Error,
            checker.codes.immutable_inout,
            "assignment target is not a mutable place",
            target_span,
        )
        .unwrap();
        expected
            .add_label(
                &fixture.sources,
                checker.symbol_span(target_symbol).unwrap(),
                "immutable binding declared here",
            )
            .unwrap();
        assert_eq!(*diagnostic, expected, "preserve the full real diagnostic");
        assert_eq!(
            checker.formed_owned_locals,
            [prefix, prior],
            "only RHS suffix rolls back before any aggregate clear"
        );
        assert!(!checker.formed_owned_locals.contains(&rhs));
        Facts::read(checker)
    });
    assert_eq!(rejected.loans, baseline.loans[..1]);
    assert_eq!(rejected.formed, baseline.formed[..2]);

    // Supplement the actual check_assignment path: replay all nine saved lengths.
    // The remaining empty fact classes are compared exactly, not claimed as nonempty coverage.
    with_checker(&fixture, &parts, false, |checker| {
        assert!(checker.diagnostics.is_empty());
        assert_eq!(Facts::read(checker), baseline);
        let lengths = rejected.lengths();
        assert_eq!(lengths.8, 2);
        assert_eq!(checker.formed_owned_locals.len(), 3);
        checker.rollback_facts(lengths);
        assert_eq!(
            Facts::read(checker),
            rejected,
            "all eight prior fact classes and the ninth history length are restored"
        );
        assert!(checker.diagnostics.is_empty());
    });
}
