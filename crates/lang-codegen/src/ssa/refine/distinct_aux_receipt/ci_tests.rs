//! CI-only controls over genuine two-owner append events. Compile locally, never execute.
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
    SNAPSHOTS.with(|v| {
        if let Some(v) = v.borrow_mut().as_mut() {
            for receipt in receipts {
                v.push(Snapshot {
                    plans: plans.clone(),
                    function: function.clone(),
                    receipt: receipt.clone(),
                    span,
                });
            }
        }
    });
}
fn capture(second: bool) -> (Snapshot, UnitLoanActionObservation) {
    let (params, args) = if second {
        ("auxiliary: String, source: String", "auxiliary, owner")
    } else {
        ("source: String, auxiliary: String", "owner, auxiliary")
    };
    let text = format!(
        "fun view({params}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; val auxiliary = \"other\"; borrow val item = view({args}); consume(item) }}"
    );
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "ci-distinct-aux.ko", &text);
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
    SNAPSHOTS.with(|v| *v.borrow_mut() = Some(Vec::new()));
    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
        &sources, &inputs, &names, &types, &typed, &owned, entry,
    )
    .unwrap();
    let mut snapshots = SNAPSHOTS.with(|v| v.borrow_mut().take().unwrap());
    assert_eq!(snapshots.len(), 1);
    (
        snapshots.pop().unwrap(),
        owned.ownership().loan_action_observations()[0].clone(),
    )
}
fn current(s: &Snapshot) -> BTreeMap<Slot, EntityId> {
    slots(&s.plans.entries[0])
        .into_iter()
        .map(Option::unwrap)
        .zip(s.receipt.entities.iter().copied())
        .collect()
}
fn replay(s: &Snapshot, count: usize) -> Result<Emitted, LoweringError> {
    let mut emitted = Emitted::new(&s.plans, &s.function, s.span)?;
    let current = current(s);
    for (role, id) in s.receipt.instructions.iter().take(count).enumerate() {
        emitted.statement(
            &s.plans,
            Tag {
                plan: 0,
                role: role.try_into().unwrap(),
            },
            &s.function,
            *id,
            &current,
            s.span,
        )?;
    }
    Ok(emitted)
}
#[test]
fn ci_only_distinct_aux_requires_catalog_identity_and_all_authorities() {
    let (s, catalog) = capture(false);
    let (_, foreign) = capture(false);
    let region = &catalog.distinct_auxiliary_binding_regions()[0];
    let ids = region.action_ids().collect::<Vec<_>>();
    super::super::admission::catalog_actions(&catalog, region, &ids, s.span).unwrap();
    assert!(super::super::admission::catalog_actions(&foreign, region, &ids, s.span).is_err());
    let foreign_ids = foreign.distinct_auxiliary_binding_regions()[0]
        .action_ids()
        .collect::<Vec<_>>();
    assert!(
        super::super::admission::catalog_actions(&catalog, region, &foreign_ids, s.span).is_err()
    );
    for fault in 0..13 {
        let mut p = s.plans.clone();
        let e = &mut p.entries[0];
        match fault {
            0 => e.activated = false,
            1 => e.source_authorized = false,
            2 => e.auxiliary_authorized = false,
            3 => e.committed = false,
            4 => e.consumer_authorized = false,
            5 => e.retired = false,
            6 => e.drop_authorized = false,
            7 => e.auxiliary_drop_authorized = false,
            8 => e.completed = false,
            9 => e.auxiliary_owner = e.owner,
            10 => e.auxiliary_root = e.root,
            11 => e.auxiliary_drop_index = e.drop_index,
            _ => e.source_index = 2,
        }
        assert!(
            Emitted::new(&p, &s.function, s.span).is_err(),
            "fault {fault}"
        );
    }
}
#[test]
fn ci_only_distinct_aux_freezes_two_owner_chronology_for_both_source_indices() {
    for second in [false, true] {
        let (s, _) = capture(second);
        replay(&s, 11)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        let p = &s.plans.entries[0];
        assert_eq!(p.source_index, usize::from(second));
        assert_ne!(s.receipt.entities[0], s.receipt.entities[1]);
        for (role, index) in [
            (0, if second { 4 } else { 2 }),
            (2, if second { 2 } else { 4 }),
        ] {
            assert_eq!(
                s.function
                    .instruction(s.receipt.instructions[role])
                    .unwrap()
                    .results,
                [s.receipt.entities[index]]
            );
        }
        let mut wrong = s.clone();
        wrong.plans.entries[0].source_index = usize::from(!second);
        assert!(replay(&wrong, 1).is_err());
        // Rebinding an already captured owner cannot pass an instruction-free alias hook.
        // Even the owner whose RootPlace has not yet appended is fixed at role zero.
        let window = replay(&s, 1).unwrap();
        let mut window_current = current(&s);
        let later_owner = if second { p.owner } else { p.auxiliary_owner };
        window_current.insert(
            later_owner.unwrap(),
            s.receipt.entities[usize::from(second)],
        );
        assert!(window.alias(&s.plans, &window_current, s.span).is_err());
        let e = replay(&s, 4).unwrap();
        let mut wrong_current = current(&s);
        wrong_current.insert(p.auxiliary_owner.unwrap(), s.receipt.entities[0]);
        assert!(e.alias(&s.plans, &wrong_current, s.span).is_err());
        let mut e = replay(&s, 2).unwrap();
        let mut wrong_current = current(&s);
        let first_owner = if second { p.auxiliary_owner } else { p.owner };
        wrong_current.insert(
            first_owner.unwrap(),
            s.receipt.entities[usize::from(!second)],
        );
        assert!(
            e.statement(
                &s.plans,
                Tag { plan: 0, role: 2 },
                &s.function,
                s.receipt.instructions[2],
                &wrong_current,
                s.span
            )
            .is_err()
        );
    }
}
#[test]
fn ci_only_distinct_aux_rejects_drop_swap_missing_end_and_noncontiguous_roles() {
    let (s, _) = capture(false);
    let current = current(&s);
    for (prefix, role, id) in [(6, 6, 10), (10, 10, 6), (5, 5, 9), (7, 7, 6)] {
        let mut e = replay(&s, prefix).unwrap();
        assert!(
            e.statement(
                &s.plans,
                Tag { plan: 0, role },
                &s.function,
                s.receipt.instructions[id],
                &current,
                s.span
            )
            .is_err()
        );
    }
    for count in [5, 6, 7, 10] {
        assert!(
            replay(&s, count)
                .unwrap()
                .finish(&s.plans, &s.function, s.span)
                .is_err()
        );
    }
    let mut e = replay(&s, 11).unwrap();
    e.receipts[0].instructions.swap(6, 10);
    assert!(e.finish(&s.plans, &s.function, s.span).is_err());
    let (foreign, _) = capture(false);
    let mut e = replay(&s, 11).unwrap();
    e.receipts[0].ids = foreign.receipt.ids;
    assert!(e.finish(&s.plans, &s.function, s.span).is_err());
    let mut e = replay(&s, 11).unwrap();
    e.receipts[0].entities.swap(0, 1);
    assert!(e.finish(&s.plans, &s.function, s.span).is_err());
}
#[test]
fn ci_only_distinct_aux_publication_rolls_back_before_original_verifier() {
    let (s, _) = capture(false);
    let (_, receipts) = observe(|| {
        let _guard = UnitPublication::begin();
        replay(&s, 11)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        Err::<(), ()>(())
    });
    assert!(receipts.is_empty());
    let (_, receipts) = observe(|| {
        let guard = UnitPublication::begin();
        replay(&s, 11)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        guard.commit();
    });
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].instructions.len(), 11);
    assert_eq!(receipts[0].entities.len(), 7);
}
