//! One-shot corruption at the actual final verifier boundary; never executed locally.
use super::*;
use crate::ssa::model::Program;
use std::cell::Cell;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn before_verifier(program: &mut Program) {
    if !ARMED.with(|armed| armed.replace(false)) {
        return;
    }
    let caller = RECEIPTS.with(|slot| {
        let slot = slot.borrow();
        let observation = slot
            .as_ref()
            .expect("fault requires actual receipt capture");
        assert_eq!(observation.pending.len(), 2);
        assert!(observation.published.is_empty());
        observation.pending[0].caller
    });
    let function = program
        .modules
        .iter_mut()
        .flat_map(|m| &mut m.functions)
        .find(|f| f.id == caller)
        .expect("real pending caller exists");
    let entry = function.entry_block().expect("real entry");
    let block = function.blocks.get_mut(entry.index()).unwrap();
    assert!(block.terminator.is_some());
    block.terminator = None;
}

#[cfg(feature = "loan-observation-test-support")]
fn armed<T>(lower: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ARMED.with(|armed| armed.set(false));
        }
    }
    ARMED.with(|armed| assert!(!armed.replace(true), "no nested fault"));
    let _reset = Reset;
    let result = lower();
    ARMED.with(|armed| assert!(!armed.get(), "actual verifier hook must consume fault"));
    result
}

#[cfg(feature = "loan-observation-test-support")]
fn verifier_result(snapshot: &str) -> &str {
    snapshot
        .strip_prefix("SECTION final.verify_result bytes=")
        .expect("actual verifier section must be first")
        .split_once('\n')
        .expect("verifier section header line")
        .1
        .split_once("\nEND_SECTION final.verify_result\n")
        .expect("complete actual verifier section")
        .0
        .trim()
}

#[cfg(feature = "loan-observation-test-support")]
#[test]
fn ordinary_verifier_result_parser_excludes_program_text() {
    let error = "SECTION final.verify_result bytes=anything\nErr(\n MissingTerminator,\n)\nEND_SECTION final.verify_result\nSECTION final.ssa_modules bytes=10\nOk(fake)\n";
    let success = "SECTION final.verify_result bytes=8\nOk(\n (),\n)\nEND_SECTION final.verify_result\nSECTION final.ssa_modules bytes=40\nErr(MissingTerminator)\n";
    assert_eq!(verifier_result(error), "Err(\n MissingTerminator,\n)");
    assert_eq!(verifier_result(success), "Ok(\n (),\n)");
    assert!(!verifier_result(success).contains("MissingTerminator"));
}

#[cfg(feature = "loan-observation-test-support")]
#[test]
#[ignore = "CI-only actual orchestrate verifier failure and atomic rollback; compile/list locally"]
fn actual_unit_verifier_error_revokes_two_pending_receipts_and_recovers() {
    use crate::ssa::unit_lower_test_support;
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun first(source:String,flag:Boolean):Unit {while(flag){borrow val item=view(source);consume(item)}}\nfun second(source:String,flag:Boolean):Unit {while(flag){borrow val item=view(source);consume(item);consume(item)}}\nfun run(source:String,flag:Boolean):Unit {first(source,flag);second(source,flag)}";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "publication-ci.ko", text);
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
    for fault in [true, false] {
        let lower = || {
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
        };
        let ((result, verified), receipts, trace) = observe_publication(|| {
            crate::ssa::unit_lower::observe_final_verifier(|| {
                if fault { armed(lower) } else { lower() }
            })
        });
        assert_eq!(verified.len(), 1, "original verifier called exactly once");
        let verification = verifier_result(&verified[0]);
        assert_eq!(trace.len(), 4);
        assert_eq!(trace[0].stage, PublicationStage::Recorded);
        assert_eq!(trace[1].stage, PublicationStage::Recorded);
        assert_eq!(trace[2].stage, PublicationStage::BeforeVerifier);
        assert_eq!(trace[0].pending.len(), 1);
        assert_eq!(trace[1].pending.len(), 2);
        assert_eq!(trace[0].pending[0], trace[1].pending[0]);
        assert_ne!(trace[1].pending[0], trace[1].pending[1]);
        assert_eq!(trace[2].pending, trace[1].pending);
        assert!(trace[..3].iter().all(|e| e.published.is_empty()));
        assert!(trace[3].pending.is_empty());
        if fault {
            let error = result
                .err()
                .expect("original verifier must reject actual malformed Program");
            assert_eq!(
                error.kind,
                crate::ssa::lowering_support::LoweringErrorKind::InvalidModel
            );
            assert!(
                error.span.is_none(),
                "original final verifier error mapping"
            );
            assert!(verification.starts_with("Err("));
            assert!(verification.contains("MissingTerminator"));
            assert_eq!(trace[3].stage, PublicationStage::RolledBack);
            assert!(trace[3].published.is_empty());
            assert!(receipts.is_empty());
        } else {
            let (program, _) = result.unwrap();
            assert!(verification.starts_with("Ok("));
            assert!(!verification.contains("MissingTerminator"));
            assert_eq!(trace[3].stage, PublicationStage::Committed);
            assert_eq!(trace[3].published, trace[2].pending);
            assert_eq!(
                receipts.iter().map(|r| r.caller).collect::<Vec<_>>(),
                trace[3].published
            );
            assert_eq!(receipts.len(), 2);
            for receipt in receipts {
                let function = program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .find(|f| f.id == receipt.caller)
                    .unwrap();
                receipt.validate(function).unwrap();
            }
        }
    }
}
