//! Ordinary finite programs only: region boundaries, not whole-function instruction shapes.
use super::*;
use crate::ssa::refine::loan_receipt;

#[test]
fn int_consumer_with_neighboring_plain_calls_has_local_receipt() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Int = 7\nfun ping(): Unit {}\nfun run(): Unit { val owner = \"kept\"; ping(); consume(view(owner)); ping() }";
    check(text, "run", 1, true);
}

#[test]
fn partial_live_source_preserves_legacy_lowering_without_receipt() {
    let text = SOURCE.replace(
        "consume(view(owner))",
        "consume(view(owner)); consume(owner)",
    );
    check(&text, "run", 0, false);
}

#[test]
fn sibling_callables_keep_catalog_and_ssa_receipts_local() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun first(): Unit { val owner = \"kept\"; consume(view(owner)) }\nfun second(): Unit { val owner = \"kept\"; consume(view(owner)) }\nfun run(): Unit { first(); second() }";
    check(text, "run", 2, false);
}

fn check(text: &str, entry_name: &str, expected: usize, neighbors: bool) {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "receipt.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "receipt.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let regions = owned
            .ownership()
            .loan_action_observations()
            .iter()
            .flat_map(|observation| {
                observation
                    .regions()
                    .iter()
                    .filter(|region| region.protocol_version() == 4)
                    .map(move |region| (observation, region))
            })
            .collect::<Vec<_>>();
        if expected == 0 {
            assert_eq!(regions.len(), 1);
            assert_eq!(regions[0].1.coverage(), UnitLoanActionCoverage::Partial);
        } else {
            assert_eq!(regions.len(), expected);
            assert!(
                regions
                    .iter()
                    .all(|(_, r)| r.coverage() == UnitLoanActionCoverage::Complete)
            );
        }
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == entry_name)
            .unwrap()
            .id();
        let (lowered, receipts) = loan_receipt::observe(|| {
            super::super::unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
        });
        let (program, _) = lowered.unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        assert_eq!(receipts.len(), expected);
        for receipt in &receipts {
            let (observation, region) = regions
                .iter()
                .find(|(_, r)| r.call() == receipt.call)
                .unwrap();
            assert_eq!(Some(receipt.callable), region.callable());
            assert_eq!(receipt.ids, region.action_ids().collect::<Vec<_>>());
            assert!(
                receipt
                    .ids
                    .iter()
                    .all(|id| observation.action(id).is_some())
            );
            let function = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.id() == receipt.caller)
                .unwrap();
            let instructions = receipt
                .instructions
                .iter()
                .map(|id| function.instruction(*id).unwrap())
                .collect::<Vec<_>>();
            let [owner, place, source, result] = receipt.entities.as_slice() else {
                panic!("four local roles")
            };
            assert_ne!(source, result);
            assert_eq!(instructions[0].operation.entities(), [*owner]);
            assert_eq!(instructions[0].results, [*place]);
            assert_eq!(instructions[1].operation.entities(), [*place]);
            assert_eq!(instructions[1].results, [*source]);
            assert!(matches!(
                instructions[2].operation,
                Operation::BorrowCall { .. }
            ));
            assert_eq!(instructions[2].results, [*result]);
            assert!(matches!(
                instructions[3].operation,
                Operation::DirectCall { .. }
            ));
            assert_eq!(instructions[3].operation.entities(), [*result]);
            assert!(matches!(
                instructions[4].operation,
                Operation::BorrowEnd { .. }
            ));
            assert_eq!(instructions[4].operation.entities(), [*result]);
            assert!(matches!(
                instructions[5].operation,
                Operation::BorrowEnd { .. }
            ));
            assert_eq!(instructions[5].operation.entities(), [*source]);
            assert!(matches!(instructions[6].operation, Operation::Drop { .. }));
            assert_eq!(instructions[6].operation.entities(), [*owner]);
            if neighbors {
                assert!(function.instructions.len() > 8);
                assert_eq!(
                    instructions[3].results.len(),
                    1,
                    "outer Int result is an ordinary value"
                );
                assert!(matches!(instructions[3].results[0], EntityId::Value(_)));
            }
        }
        if let [first, second] = receipts.as_slice() {
            assert_ne!(first.call, second.call);
            assert_ne!(first.callable, second.callable);
            assert_ne!(first.caller, second.caller);
            assert!(first.ids.iter().all(|id| !second.ids.contains(id)));
            assert!(
                first
                    .entities
                    .iter()
                    .all(|id| !second.entities.contains(id))
            );
        }
    }
}
