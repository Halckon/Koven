//! CI-only corruption controls. Local checks compile this module without executing it.
use super::*;
use crate::ssa::unit_lower_test_support;
use lang_frontend::{
    name_resolution::SourceUnitInput, ownership_checking::UnitLoanActionObservation,
    source::SourceMap, type_checking::standard_environments,
};
use std::cell::RefCell;
#[derive(Clone)]
struct Snapshot {
    plans: Plans,
    function: Function,
    receipt: Receipt,
    span: Span,
}
thread_local! { static SNAPSHOTS: RefCell<Option<Vec<Snapshot>>> = const { RefCell::new(None) }; }
pub(super) fn record(plans: &Plans, function: &Function, receipts: &[Receipt], span: Span) {
    SNAPSHOTS.with(|output| {
        if let Some(output) = output.borrow_mut().as_mut() {
            for receipt in receipts {
                output.push(Snapshot {
                    plans: plans.clone(),
                    function: function.clone(),
                    receipt: receipt.clone(),
                    span,
                });
            }
        }
    });
}
fn capture() -> (Snapshot, UnitLoanActionObservation) {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(
        &mut sources,
        "ci-binding.ko",
        "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner); consume(item) }",
    );
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap()
        .id();
    SNAPSHOTS.with(|output| *output.borrow_mut() = Some(Vec::new()));
    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
        &sources, &inputs, &names, &types, &typed, &owned, entry,
    )
    .unwrap();
    let mut output = SNAPSHOTS.with(|output| output.borrow_mut().take().unwrap());
    assert_eq!(output.len(), 1);
    (
        output.pop().unwrap(),
        owned.ownership().loan_action_observations()[0].clone(),
    )
}
fn replay(snapshot: &Snapshot, count: usize, rebind: bool) -> Result<Emitted, LoweringError> {
    let mut emitted = Emitted::new(&snapshot.plans, &snapshot.function, snapshot.span)?;
    let p = &snapshot.plans.entries[0];
    let mut current: BTreeMap<_, _> = [p.owner, p.place, p.source, p.result]
        .into_iter()
        .map(Option::unwrap)
        .zip(snapshot.receipt.entities.iter().copied())
        .collect();
    for (role, id) in snapshot.receipt.instructions.iter().take(count).enumerate() {
        if rebind && role == 4 {
            current.insert(p.source.unwrap(), snapshot.receipt.entities[3]);
        }
        emitted.statement(
            &snapshot.plans,
            Tag {
                plan: 0,
                role: u8::try_from(role).unwrap(),
            },
            &snapshot.function,
            *id,
            &current,
            snapshot.span,
        )?;
    }
    Ok(emitted)
}
#[test]
fn ci_only_binding_rejects_foreign_catalog_and_action_ids() {
    let (snapshot, first) = capture();
    let (_, foreign) = capture();
    let region = &first.binding_regions()[0];
    let ids = region.action_ids().collect::<Vec<_>>();
    assert!(super::super::catalog_actions(&first, region, &ids, snapshot.span).is_ok());
    assert!(super::super::catalog_actions(&foreign, region, &ids, snapshot.span).is_err());
    let foreign_ids = foreign.binding_regions()[0]
        .action_ids()
        .collect::<Vec<_>>();
    assert!(super::super::catalog_actions(&first, region, &foreign_ids, snapshot.span).is_err());
}
#[test]
fn ci_only_binding_rejects_missing_completion_stage_and_rebound_entity() {
    let (snapshot, _) = capture();
    replay(&snapshot, 7, false)
        .unwrap()
        .finish(&snapshot.plans, &snapshot.function, snapshot.span)
        .unwrap();
    assert!(
        replay(&snapshot, 6, false)
            .unwrap()
            .finish(&snapshot.plans, &snapshot.function, snapshot.span)
            .is_err()
    );
    assert!(replay(&snapshot, 7, true).is_err());
    let mut missing = snapshot.clone();
    missing.plans.entries[0].completed = false;
    assert!(Emitted::new(&missing.plans, &missing.function, missing.span).is_err());
    missing.plans.entries[0].completed = true;
    missing.plans.entries[0].stage = 6;
    assert!(Emitted::new(&missing.plans, &missing.function, missing.span).is_err());
}
#[test]
fn ci_only_binding_rejects_wrong_consumer_and_retirement_identity() {
    let (snapshot, _) = capture();
    let mut plans = snapshot.plans.clone();
    let p = plans.entries[0].clone();
    plans.entries[0].stage = 3;
    plans.entries[0].consumer_authorized = false;
    assert!(
        plans
            .consumer_argument(
                p.consumer_call,
                p.consumer_argument,
                p.root,
                p.result.unwrap(),
                snapshot.span
            )
            .is_err()
    );
    plans.entries[0].stage = 4;
    plans.entries[0].retired = false;
    assert!(
        plans
            .retire(
                p.binding,
                p.end_point,
                p.source,
                Some(p.source.as_slice()),
                snapshot.span
            )
            .is_err()
    );
    assert!(
        plans
            .retire(p.binding, p.end_point, p.result, Some(&[]), snapshot.span)
            .is_err()
    );
}
