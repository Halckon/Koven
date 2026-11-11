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
fn capture(second: bool, count: usize, prefix: usize) -> (Snapshot, UnitLoanActionObservation) {
    let (params, args) = if second {
        ("auxiliary: String, source: String", "auxiliary, owner")
    } else {
        ("source: String, auxiliary: String", "owner, auxiliary")
    };
    let before = "consume(auxiliary);".repeat(prefix);
    let suffix = "consume(auxiliary);".repeat(count - prefix);
    let text = format!(
        "fun view({params}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; val auxiliary = \"other\"; borrow val item = view({args}); {before} consume(item); {suffix} }}"
    );
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "ci-interleaved-aux.ko", &text);
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
            Tag { plan: 0, role },
            &s.function,
            *id,
            &current,
            s.span,
        )?;
    }
    Ok(emitted)
}
#[test]
fn ci_only_interleaved_requires_owned_catalog_and_every_phase_authority() {
    let (s, catalog) = capture(false, 2, 1);
    let (_, foreign) = capture(false, 2, 1);
    let region = &catalog.interleaved_auxiliary_regions()[0];
    let ids = region.action_ids().collect::<Vec<_>>();
    super::super::admission::catalog_actions(&catalog, region, &ids, s.span).unwrap();
    assert!(super::super::admission::catalog_actions(&foreign, region, &ids, s.span).is_err());
    let foreign_ids = foreign.interleaved_auxiliary_regions()[0]
        .action_ids()
        .collect::<Vec<_>>();
    assert!(
        super::super::admission::catalog_actions(&catalog, region, &foreign_ids, s.span).is_err()
    );
    for fault in 0..22 {
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
            7 => p.auxiliary_drop_authorized = false,
            8 => p.completed = false,
            9 => p.uses[0].authorized = false,
            10 => p.uses[1].authorized = false,
            11 => p.owner = p.auxiliary_owner,
            12 => p.root = p.auxiliary_root,
            13 => p.drop_index = p.auxiliary_drop_index,
            14 => p.source_index = 2,
            15 => p.uses[1].place = p.uses[0].place,
            16 => p.uses[1].loan = p.uses[0].loan,
            17 => p.prefix = 0,
            18 => p.prefix = 3,
            19 => p.stage -= 1,
            20 => p.uses.clear(),
            _ => {
                let u = p.uses[0].clone();
                p.uses.resize(9, u);
            }
        }
        assert!(
            Emitted::new(&plans, &s.function, s.span).is_err(),
            "fault {fault}"
        );
    }
}
#[test]
fn ci_only_interleaved_preserves_dynamic_order_and_all_fresh_loans() {
    for count in 1..=8 {
        for prefix in 1..=count {
            for second in [false, true] {
                let (s, _) = capture(second, count, prefix);
                let p = &s.plans.entries[0];
                let roles = 11 + 4 * count;
                replay(&s, roles)
                    .unwrap()
                    .finish(&s.plans, &s.function, s.span)
                    .unwrap();
                assert_eq!(s.receipt.entities.len(), 7 + 2 * count);
                let fixed = replay(&s, 1).unwrap();
                let mut mapping = current(&s);
                mapping.insert(
                    if second { p.owner } else { p.auxiliary_owner }.unwrap(),
                    s.receipt.entities[usize::from(second)],
                );
                assert!(fixed.alias(&s.plans, &mapping, s.span).is_err());
                for (index, u) in p.uses.iter().enumerate() {
                    let fixed = replay(&s, p.use_role(index) + 2).unwrap();
                    for (slot, wrong) in [
                        (u.place, s.receipt.entities[4]),
                        (u.loan, s.receipt.entities[5]),
                    ] {
                        let mut mapping = current(&s);
                        mapping.insert(slot.unwrap(), wrong);
                        assert!(fixed.alias(&s.plans, &mapping, s.span).is_err());
                    }
                }
                let mut swapped = s.clone();
                swapped.plans.entries[0].source_index = usize::from(!second);
                assert!(replay(&swapped, 1).is_err());
            }
        }
    }
}
#[test]
fn ci_only_interleaved_rejects_missing_reordered_and_foreign_events() {
    for (count, prefix) in [(1, 1), (3, 1), (3, 3)] {
        let (s, _) = capture(false, count, prefix);
        let p = &s.plans.entries[0];
        let roles = 11 + 4 * count;
        for length in 0..roles {
            assert!(
                replay(&s, length)
                    .unwrap()
                    .finish(&s.plans, &s.function, s.span)
                    .is_err()
            );
        }
        for (role, wrong) in [
            (p.source_drop_role(), p.auxiliary_drop_role()),
            (p.auxiliary_drop_role(), p.source_drop_role()),
            (p.consumer_role(), p.use_role(0) + 2),
            (p.use_role(0) + 3, 5),
        ] {
            let mut e = replay(&s, role).unwrap();
            assert!(
                e.statement(
                    &s.plans,
                    Tag { plan: 0, role },
                    &s.function,
                    s.receipt.instructions[wrong],
                    &current(&s),
                    s.span
                )
                .is_err()
            );
        }
        let mut e = replay(&s, roles).unwrap();
        e.receipts[0]
            .instructions
            .swap(p.source_drop_role(), p.auxiliary_drop_role());
        assert!(e.finish(&s.plans, &s.function, s.span).is_err());
        let (foreign, _) = capture(false, count, prefix);
        let mut e = replay(&s, roles).unwrap();
        e.receipts[0].ids = foreign.receipt.ids;
        assert!(e.finish(&s.plans, &s.function, s.span).is_err());
        let mut e = replay(&s, roles).unwrap();
        e.receipts[0].entities.swap(5, 8);
        assert!(e.finish(&s.plans, &s.function, s.span).is_err());
        let mut e = replay(&s, roles).unwrap();
        e.receipts[0].prefix = 0;
        assert!(e.finish(&s.plans, &s.function, s.span).is_err());
    }
}
#[test]
fn ci_only_interleaved_rolls_back_before_original_verifier_publication() {
    let (s, _) = capture(false, 1, 1);
    let (_, receipts) = observe(|| {
        let _guard = UnitPublication::begin();
        replay(&s, 15)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        Err::<(), ()>(())
    });
    assert!(receipts.is_empty());
    let (_, receipts) = observe(|| {
        let guard = UnitPublication::begin();
        replay(&s, 15)
            .unwrap()
            .finish(&s.plans, &s.function, s.span)
            .unwrap();
        guard.commit();
    });
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].instructions.len(), 15);
    assert_eq!(receipts[0].entities.len(), 9);
}

#[test]
fn ci_only_interleaved_rejects_unbounded_prefix_before_role_arithmetic() {
    let (s, _) = capture(false, 2, 1);
    let original = &s.plans.entries[0];
    let corrupted = || {
        let mut plans = s.plans.clone();
        plans.entries[0].prefix = usize::MAX;
        plans
    };
    let mut plans = corrupted();
    plans.entries[0].stage = original.consumer_role();
    plans.entries[0].consumer_authorized = false;
    assert!(
        plans
            .consumer_argument(
                original.consumer_call,
                original.consumer_argument,
                original.binding,
                original.result.unwrap(),
                s.span
            )
            .is_err()
    );
    let mut plans = corrupted();
    plans.entries[0].stage = original.consumer_role() + 1;
    plans.entries[0].retired = false;
    assert!(
        plans
            .retire(
                original.binding,
                original.end_point,
                original.result,
                Some(original.source.as_slice()),
                s.span
            )
            .is_err()
    );
    for (index, root, owner, stage) in [
        (
            original.drop_index,
            original.root,
            original.owner,
            original.source_drop_role(),
        ),
        (
            original.auxiliary_drop_index,
            original.auxiliary_root,
            original.auxiliary_owner,
            original.auxiliary_drop_role(),
        ),
    ] {
        let mut plans = corrupted();
        plans.entries[0].stage = stage;
        plans.entries[0].drop_authorized = false;
        plans.entries[0].auxiliary_drop_authorized = false;
        assert!(
            plans
                .drop_fact(index, UnitDropTarget::Named(root), owner.unwrap(), s.span)
                .is_err()
        );
    }
    let mut plans = corrupted();
    plans.entries[0].stage = 0;
    plans.entries[0].source_authorized = false;
    assert!(
        plans
            .source_operand(original.source_call, original.source_argument, s.span)
            .is_err()
    );
    let mut plans = s.plans.clone();
    plans.entries[0].source_index = usize::MAX;
    plans.entries[0].stage = 0;
    plans.entries[0].source_authorized = false;
    assert!(
        plans
            .source_operand(original.source_call, original.source_argument, s.span)
            .is_err()
    );
}
