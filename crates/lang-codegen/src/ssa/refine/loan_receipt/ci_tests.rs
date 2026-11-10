//! CI-only synthetic corruption. Local validation compiles these tests but never runs them.
use super::*;
use crate::ssa::{model::Origin, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
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

pub(super) fn record(plans: &Plans, function: &Function, receipts: &[Receipt]) {
    SNAPSHOTS.with(|snapshots| {
        if let Some(snapshots) = snapshots.borrow_mut().as_mut() {
            for receipt in receipts {
                let Origin::Source(span) = function.origin else {
                    panic!("source fixture")
                };
                snapshots.push(Snapshot {
                    plans: plans.clone(),
                    function: function.clone(),
                    receipt: receipt.clone(),
                    span,
                });
            }
        }
    });
}

fn capture() -> Snapshot {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; consume(view(owner)) }";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "ci-receipt.ko", text);
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
    SNAPSHOTS.with(|snapshots| *snapshots.borrow_mut() = Some(Vec::new()));
    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
        &sources, &inputs, &names, &types, &typed, &owned, entry,
    )
    .unwrap();
    let mut snapshots = SNAPSHOTS.with(|snapshots| snapshots.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    snapshots.pop().unwrap()
}

/// Recheck exact actual append identities. This never emits or reconstructs SSA operations.
fn checked(
    snapshot: &Snapshot,
    count: usize,
    duplicate: bool,
    rebind: bool,
) -> Result<Emitted, LoweringError> {
    let mut emitted = Emitted::new(&snapshot.plans, &snapshot.function, snapshot.span)?;
    let plan = &snapshot.plans.entries[0];
    let mut current: BTreeMap<_, _> = [plan.owner, plan.place, plan.source, plan.result]
        .into_iter()
        .map(Option::unwrap)
        .zip(snapshot.receipt.entities.iter().copied())
        .collect();
    for (role, id) in snapshot.receipt.instructions.iter().take(count).enumerate() {
        if rebind && role == 4 {
            current.insert(plan.source.unwrap(), snapshot.receipt.entities[3]);
        }
        let tag = Tag {
            plan: 0,
            role: u8::try_from(role).unwrap(),
        };
        emitted.statement(
            &snapshot.plans,
            tag,
            &snapshot.function,
            *id,
            &current,
            snapshot.span,
        )?;
        if duplicate && role == 2 {
            emitted.statement(
                &snapshot.plans,
                tag,
                &snapshot.function,
                *id,
                &current,
                snapshot.span,
            )?;
        }
    }
    Ok(emitted)
}

#[test]
fn ci_only_foreign_catalog_receipt_is_rejected() {
    let first = capture();
    let second = capture();
    let mut emitted = checked(&first, 7, false, false).unwrap();
    assert_ne!(first.receipt.ids, second.receipt.ids);
    emitted.receipts[0].ids = second.receipt.ids;
    assert!(
        emitted
            .finish(&first.plans, &first.function, first.span)
            .is_err()
    );
}

#[test]
fn ci_only_duplicate_or_missing_role_is_rejected() {
    let snapshot = capture();
    assert!(checked(&snapshot, 7, true, false).is_err());
    let emitted = checked(&snapshot, 6, false, false).unwrap();
    assert!(
        emitted
            .finish(&snapshot.plans, &snapshot.function, snapshot.span)
            .is_err()
    );
    let mut plans = snapshot.plans.clone();
    plans.entries[0].stage = 6;
    assert!(
        plans
            .finish(
                &[plans.entries[0].drop_index].into_iter().collect(),
                snapshot.span
            )
            .is_err()
    );
}

#[test]
fn ci_only_rebound_slot_or_wrong_callee_is_rejected() {
    let mut snapshot = capture();
    assert!(checked(&snapshot, 7, false, true).is_err());
    let id = snapshot.receipt.instructions[3];
    let instruction = snapshot
        .function
        .instructions
        .iter_mut()
        .find(|i| i.id == id)
        .unwrap();
    let Operation::DirectCall { callee, .. } = &mut instruction.operation else {
        panic!("actual call")
    };
    *callee = snapshot.function.id;
    assert!(checked(&snapshot, 7, false, false).is_err());
}

#[test]
fn ci_only_foreign_function_instruction_is_rejected() {
    let first = capture();
    let second = capture();
    let mut snapshot = first.clone();
    snapshot.receipt.instructions[2] = second.receipt.instructions[2];
    // Function-qualified instruction identity must fail lookup, before observing an operation.
    let mut emitted = Emitted::new(&first.plans, &first.function, first.span).unwrap();
    let foreign = snapshot.receipt.instructions[2];
    assert!(
        emitted
            .statement(
                &first.plans,
                Tag { plan: 0, role: 0 },
                &first.function,
                foreign,
                &BTreeMap::new(),
                first.span
            )
            .is_err()
    );
}
