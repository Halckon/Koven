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
    capture_source(false)
}
fn capture_source(source_second: bool) -> (Snapshot, UnitLoanActionObservation) {
    let mut sources = SourceMap::new();
    let text = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }";
    let text = if source_second {
        text.replace(
            "view(source: String, aux: String)",
            "view(aux: String, source: String)",
        )
    } else {
        text.to_owned()
    };
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "ci-aux-binding.ko", &text);
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
    let mut current: BTreeMap<_, _> = [
        p.owner,
        p.place,
        p.source,
        p.auxiliary_place,
        p.auxiliary,
        p.result,
    ]
    .into_iter()
    .map(Option::unwrap)
    .zip(snapshot.receipt.entities.iter().copied())
    .collect();
    for (role, id) in snapshot.receipt.instructions.iter().take(count).enumerate() {
        if rebind && role == 6 {
            current.insert(p.source.unwrap(), snapshot.receipt.entities[5]);
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
fn ci_only_aux_binding_rejects_foreign_catalog_and_action_ids() {
    let (snapshot, first) = capture();
    let (_, foreign) = capture();
    let region = &first.auxiliary_binding_regions()[0];
    let ids = region.action_ids().collect::<Vec<_>>();
    assert!(super::super::admission::catalog_actions(&first, region, &ids, snapshot.span).is_ok());
    assert!(
        super::super::admission::catalog_actions(&foreign, region, &ids, snapshot.span).is_err()
    );
    let foreign_ids = foreign.auxiliary_binding_regions()[0]
        .action_ids()
        .collect::<Vec<_>>();
    assert!(
        super::super::admission::catalog_actions(&first, region, &foreign_ids, snapshot.span)
            .is_err()
    );
}
#[test]
fn ci_only_aux_binding_rejects_missing_completion_stage_and_rebound_entity() {
    let (snapshot, _) = capture();
    replay(&snapshot, 10, false)
        .unwrap()
        .finish(&snapshot.plans, &snapshot.function, snapshot.span)
        .unwrap();
    assert!(
        replay(&snapshot, 9, false)
            .unwrap()
            .finish(&snapshot.plans, &snapshot.function, snapshot.span)
            .is_err()
    );
    assert!(replay(&snapshot, 10, true).is_err());
    let mut missing = snapshot.clone();
    missing.plans.entries[0].completed = false;
    assert!(Emitted::new(&missing.plans, &missing.function, missing.span).is_err());
    missing.plans.entries[0].completed = true;
    missing.plans.entries[0].stage = 6;
    assert!(Emitted::new(&missing.plans, &missing.function, missing.span).is_err());
}
#[test]
fn ci_only_aux_binding_rejects_wrong_consumer_and_retirement_identity() {
    let (snapshot, _) = capture();
    let mut plans = snapshot.plans.clone();
    let p = plans.entries[0].clone();
    plans.entries[0].stage = 6;
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
    plans.entries[0].stage = 7;
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

#[test]
fn ci_only_aux_binding_publication_is_atomic_on_unit_failure() {
    let (s, _) = capture();
    let (result, receipts) = observe(|| {
        let _guard = UnitPublication::begin();
        replay(&s, 10, false)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        Err::<(), ()>(())
    });
    assert!(result.is_err());
    assert!(receipts.is_empty());
    let (_, receipts) = observe(|| {
        let guard = UnitPublication::begin();
        replay(&s, 10, false)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        guard.commit();
    });
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].instructions.len(), 10);
    assert_eq!(receipts[0].entities.len(), 6);
    let (_, receipts) = observe(|| {
        let guard = UnitPublication::begin();
        replay(&s, 10, false)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        guard.commit();
        let _failed = UnitPublication::begin();
        replay(&s, 10, false)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
    });
    assert_eq!(
        receipts.len(),
        1,
        "later failure cannot leak pending receipts or erase committed unit"
    );
}

#[test]
fn ci_only_aux_binding_rejects_missing_authority_wrong_aux_and_duplicate_roles() {
    let (s, _) = capture();
    for fault in 0..8 {
        let mut plans = s.plans.clone();
        let p = &mut plans.entries[0];
        match fault {
            0 => p.activated = false,
            1 => p.source_authorized = false,
            2 => p.auxiliary_authorized = false,
            3 => p.committed = false,
            4 => p.consumer_authorized = false,
            5 => p.retired = false,
            6 => p.drop_authorized = false,
            _ => p.completed = false,
        }
        assert!(
            Emitted::new(&plans, &s.function, s.span).is_err(),
            "authority {fault}"
        );
    }
    let p = s.plans.entries[0].clone();
    let current: BTreeMap<_, _> = [
        p.owner,
        p.place,
        p.source,
        p.auxiliary_place,
        p.auxiliary,
        p.result,
    ]
    .into_iter()
    .map(Option::unwrap)
    .zip(s.receipt.entities.iter().copied())
    .collect();
    let mut e = replay(&s, 5, false).unwrap();
    // Source retirement cannot stand in for auxiliary retirement.
    assert!(
        e.statement(
            &s.plans,
            Tag { plan: 0, role: 5 },
            &s.function,
            s.receipt.instructions[8],
            &current,
            s.span
        )
        .is_err()
    );
    let mut e = replay(&s, 6, false).unwrap();
    assert!(
        e.statement(
            &s.plans,
            Tag { plan: 0, role: 5 },
            &s.function,
            s.receipt.instructions[5],
            &current,
            s.span
        )
        .is_err()
    );
    let e = replay(&s, 6, false).unwrap();
    let mut rebound = current.clone();
    rebound.insert(p.auxiliary.unwrap(), s.receipt.entities[2]);
    assert!(e.alias(&s.plans, &rebound, s.span).is_err());
    let mut plans = s.plans.clone();
    plans.entries[0].stage = 2;
    plans.entries[0].auxiliary_authorized = false;
    assert!(
        plans
            .source_operand(p.source_call, p.source_argument, s.span)
            .is_err()
    );
    plans
        .source_operand(p.source_call, p.auxiliary_argument, s.span)
        .unwrap();
    assert!(
        plans
            .source_operand(p.source_call, p.auxiliary_argument, s.span)
            .is_err()
    );
    let mut plans = s.plans.clone();
    assert!(
        plans
            .enter(
                p.entry_statement,
                &BTreeMap::from([(p.root, p.owner.unwrap())]),
                p.block.unwrap(),
                s.span
            )
            .is_err()
    );
    assert!(
        plans
            .complete(
                p.completion_statement,
                &BTreeMap::from([(p.root, p.owner.unwrap())]),
                &BTreeSet::from([p.root]),
                &BTreeSet::from([p.drop_index]),
                p.block.unwrap(),
                s.span
            )
            .is_err()
    );
}

#[test]
fn ci_only_aux_binding_rejects_forged_source_second_chronology() {
    let (snapshot, catalog) = capture_source(true);
    let plan = &snapshot.plans.entries[0];
    assert_eq!(plan.source_index, 1);
    let region = &catalog.auxiliary_binding_regions()[0];
    assert_eq!(
        snapshot.receipt.ids,
        region.action_ids().collect::<Vec<_>>()
    );
    let first = snapshot
        .function
        .instruction(snapshot.receipt.instructions[0])
        .unwrap();
    let third = snapshot
        .function
        .instruction(snapshot.receipt.instructions[2])
        .unwrap();
    assert_eq!(first.results, [snapshot.receipt.entities[3]]);
    assert_eq!(third.results, [snapshot.receipt.entities[1]]);
    replay(&snapshot, 10, false)
        .unwrap()
        .finish(&snapshot.plans, &snapshot.function, snapshot.span)
        .unwrap();
    let mut wrong = snapshot.clone();
    wrong.plans.entries[0].source_index = 0;
    assert!(replay(&wrong, 1, false).is_err());
    wrong.plans.entries[0].source_index = 2;
    assert!(Emitted::new(&wrong.plans, &wrong.function, wrong.span).is_err());
    let mut plans = snapshot.plans.clone();
    let p = &mut plans.entries[0];
    p.stage = 0;
    p.source_authorized = false;
    p.auxiliary_authorized = false;
    assert!(
        plans
            .source_operand(plan.source_call, plan.source_argument, snapshot.span)
            .is_err()
    );
    plans
        .source_operand(plan.source_call, plan.auxiliary_argument, snapshot.span)
        .unwrap();
    assert!(
        plans
            .source_operand(plan.source_call, plan.auxiliary_argument, snapshot.span)
            .is_err()
    );
}
