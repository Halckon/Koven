//! Nonempty ordinary baseline and ignored CI-only rollback using the same source.
use super::*;
use crate::{
    llvm::render_verified_program,
    ssa::{refine::arm_loop_receipt, unit_lower_test_support},
};
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::owned_compilation_unit_view,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

fn check_publication(inject_failure: bool) {
    // Reuse the existing same-source arm-loop function bodies, not an invented producer.
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun first(source:String,flag:Boolean,choose:Boolean):Unit {while(flag){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b);consume(b)}}}\nfun second(source:String,flag:Boolean,choose:Boolean):Unit {while(flag){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b)}}}";
    for memory in [false, true] {
        let mut sources = SourceMap::new();
        let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "arms.ko", text);
        let inputs = [if memory {
            SourceUnitInput::memory_document(source, &parsed)
        } else {
            SourceUnitInput::new("root", "arms.ko", source, &parsed)
        }];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(raw.diagnostics().is_empty());
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw).unwrap();
        assert!(complete.is_same_analysis(&raw));
        let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
            .unwrap()
            .into_result()
            .unwrap();
        assert!(owned.deferred().is_empty());
        let owned = owned.validate().unwrap();
        let roots = ["first", "second"].map(|name| {
            let found = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.name() == name)
                .map(|d| d.id())
                .collect::<Vec<_>>();
            let [id] = found.as_slice() else {
                panic!("unique source root")
            };
            *id
        });
        let arena = typed.types().types().len();
        let view =
            owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned).unwrap();
        for order in [roots, [roots[1], roots[0]]] {
            if inject_failure {
                let (failed, published, trace) =
                    arm_loop_receipt::observe_final_verifier_failure(|| lower_roots(&view, &order));
                assert!(matches!(
                    failed,
                    Err(LoweringError {
                        kind: LoweringErrorKind::InvalidModel,
                        ..
                    })
                ));
                assert_eq!(trace.recorded, 2);
                assert_eq!(trace.before_verifier, 2);
                assert!(trace.rolled_back);
                assert_eq!(trace.pending_after, 0);
                assert!(
                    published.is_empty(),
                    "neither root may publish on shared failure"
                );
                assert_eq!(typed.types().types().len(), arena);
            }
            // Fresh observation on the SAME validated view must recover both roots.

            let (result, receipts, trace) =
                arm_loop_receipt::observe(|| lower_roots(&view, &order));
            let (program, ids) = result.unwrap();
            assert_eq!(ids.len(), 2);
            assert_eq!(receipts.len(), 2);
            assert_eq!(trace.recorded, 2);
            assert_eq!(trace.before_verifier, 2);
            assert!(!trace.rolled_back);
            assert_eq!(trace.pending_after, 0);
            let receipt_roots = receipts
                .iter()
                .map(|r| r.caller)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(receipt_roots, ids.values().copied().collect());
            for receipt in &receipts {
                let function = program
                    .module(receipt.caller.module())
                    .unwrap()
                    .function(receipt.caller)
                    .unwrap();
                receipt.validate(function).unwrap();
            }
            render_verified_program(&program).unwrap();
            assert_eq!(typed.types().types().len(), arena);
        }
    }
}

#[test]
fn ordinary_common_roots_publish_two_independent_arm_receipts() {
    check_publication(false);
}

#[test]
#[ignore = "CI-only real verifier corruption; local compile/list only"]
fn ci_only_common_roots_final_verifier_failure_rolls_back_and_recovers() {
    assert_eq!(
        std::env::var("GITHUB_ACTIONS").as_deref(),
        Ok("true"),
        "root-set rollback faults are CI-only"
    );
    check_publication(true);
}
