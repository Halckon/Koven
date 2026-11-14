//! Ordinary real unit orchestration; no forged receipts or injected verifier failures.
use super::*;
use crate::ssa::refine::fresh_loop_receipt::{PublicationStage, observe_publication};
use lang_frontend::ownership_checking::{check_compilation_unit_ownership, with_loan_observation};

fn check_unit(fails: bool) {
    let tail = if fails {
        r#"fun later(source:String,other:String,flag:Boolean):Unit {consume("${1}")}"#
    } else {
        "fun later(source:String,flag:Boolean):Unit {while(flag){borrow val x=view(source);consume(x);consume(x)}}"
    };
    let invoke = if fails {
        "later(source,other,flag)"
    } else {
        "later(source,flag)"
    };
    let text = format!(
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun first(source:String,flag:Boolean):Unit {{while(flag){{borrow val x=view(source);consume(x)}}}}\n{tail}\nfun run(source:String,other:String,flag:Boolean):Unit {{first(source,flag);{invoke}}}"
    );
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "publication.ko", &text);
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "publication.ko", source, &parsed),
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
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        for owned in [&on, &off] {
            let ((result, verified), receipts, trace) = observe_publication(|| {
                crate::ssa::unit_lower::observe_final_verifier(|| {
                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, owned, entry,
                    )
                })
            });
            let recorded: Vec<_> = trace
                .iter()
                .filter(|e| e.stage == PublicationStage::Recorded)
                .collect();
            assert!(
                !recorded.is_empty(),
                "must actually finish a candidate before later outcome"
            );
            assert!(recorded.iter().all(|e| e.published.is_empty()));
            assert_eq!(recorded[0].pending.len(), 1);
            if fails {
                let error = result
                    .err()
                    .expect("FE legal interpolation remains unsupported in CG");
                assert_eq!(
                    error.kind,
                    crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
                );
                let rejected = error.span.expect("actual later body error span");
                assert_eq!(sources.slice(rejected).unwrap(), "\"${1}\"");
                assert!(rejected.start() > text.find("fun later").unwrap());
                assert!(verified.is_empty());
                assert!(receipts.is_empty());
                assert_eq!(recorded.len(), 1);
                assert_eq!(
                    trace.iter().map(|e| &e.stage).collect::<Vec<_>>(),
                    vec![&PublicationStage::Recorded, &PublicationStage::RolledBack]
                );
                let last = trace.last().unwrap();
                assert_eq!(last.stage, PublicationStage::RolledBack);
                assert!(last.pending.is_empty() && last.published.is_empty());
            } else {
                let (program, _) = result.unwrap();
                assert_eq!(verified.len(), 1);
                assert_eq!(receipts.len(), 2);
                assert_eq!(recorded.len(), 2);
                assert_ne!(recorded[0].pending[0], recorded[1].pending[1]);
                let before: Vec<_> = trace
                    .iter()
                    .filter(|e| e.stage == PublicationStage::BeforeVerifier)
                    .collect();
                assert_eq!(
                    before.len(),
                    1,
                    "missing actual verifier boundary observation, not a production failure"
                );
                assert_eq!(before[0].pending, recorded[1].pending);
                assert!(before[0].published.is_empty());
                assert_eq!(
                    trace.iter().map(|e| &e.stage).collect::<Vec<_>>(),
                    vec![
                        &PublicationStage::Recorded,
                        &PublicationStage::Recorded,
                        &PublicationStage::BeforeVerifier,
                        &PublicationStage::Committed
                    ]
                );
                assert_eq!(recorded[1].pending.len(), 2);
                assert_eq!(recorded[1].pending[0], recorded[0].pending[0]);
                let last = trace.last().unwrap();
                assert_eq!(last.stage, PublicationStage::Committed);
                assert!(last.pending.is_empty());
                assert_eq!(last.published, before[0].pending);
                assert_eq!(
                    receipts.iter().map(|r| r.caller).collect::<Vec<_>>(),
                    last.published
                );
                for receipt in &receipts {
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
}

#[test]
fn ordinary_fresh_unit_publication_is_after_whole_unit_verification() {
    check_unit(false);
}

#[test]
fn ordinary_fresh_unit_later_lowering_failure_rolls_back_then_recovers() {
    check_unit(true);
    check_unit(false);
}
