//! Extra callable resources must not make optional loop receipts change execution.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn ordinary_local_loop_extra_owner_keeps_original_drop_and_observer_parity() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(flag: Boolean, own extra: String): Unit {val source=\"value\";borrow val item=view(source);while(flag){consume(item)}}";
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "extra-owner-loop.ko", text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "extra-owner-loop.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, on) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let off = with_loan_observation(false, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap()
        });
        assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let mut previous = None;
        for owned in [&on, &off] {
            let ((result, verified), receipts) = crate::ssa::refine::loop_receipt::observe(|| {
                crate::ssa::unit_lower::observe_final_verifier(|| {
                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, owned, entry,
                    )
                })
            });
            let (program, _) = result.unwrap();
            assert_eq!(verified.len(), 1);
            assert!(receipts.is_empty());
            verify_program(&program).unwrap();
            let run = program
                .modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.name.contains(".run."))
                .unwrap();
            assert_eq!(
                run.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowBegin { .. }))
                    .count(),
                1
            );
            assert_eq!(
                run.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                    .count(),
                1
            );
            assert_eq!(
                run.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
                    .count(),
                2
            );
            let drops = run
                .instructions
                .iter()
                .filter_map(|i| match i.operation {
                    Operation::Drop { owner } => Some(EntityId::Value(owner)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                drops.len(),
                2,
                "local source and unrelated callable owner retain both real Drop operations"
            );
            let extra = run.blocks[0].parameters[1];
            assert!(matches!(extra, EntityId::Value(_)));
            let mut aliases = std::collections::BTreeSet::from([extra]);
            for _ in 0..=run.blocks.len() {
                let before = aliases.len();
                for block in &run.blocks {
                    let edges = match &block.terminator.as_ref().unwrap().kind {
                        TerminatorKind::Branch(edge) => vec![edge],
                        TerminatorKind::Conditional {
                            when_true,
                            when_false,
                            ..
                        } => vec![when_true, when_false],
                        TerminatorKind::Return { .. } => vec![],
                        _ => panic!("unexpected CFG"),
                    };
                    for edge in edges {
                        let target = run.block(edge.target).unwrap();
                        for (arg, param) in edge.arguments.iter().zip(&target.parameters) {
                            if aliases.contains(arg) {
                                aliases.insert(*param);
                            }
                        }
                    }
                }
                if aliases.len() == before {
                    break;
                }
            }
            assert_eq!(
                drops.iter().filter(|id| aliases.contains(id)).count(),
                1,
                "actual extra parameter destroyed exactly once"
            );
            let rendered = format!(
                "{verified:?}\n{}",
                crate::llvm::render_verified_program(&program).unwrap()
            );
            if let Some(previous) = previous.as_ref() {
                assert_eq!(&rendered, previous);
            }
            previous = Some(rendered);
        }
    }
}
