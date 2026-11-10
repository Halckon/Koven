//! Ordinary finite original sources; no mutation, Abort, or resource-counter execution.
use super::UnitSource;
use crate::ssa::{model::TerminatorKind, verify::verify_program};
const SOURCE: &str = "fun run(flag: Boolean): String { var held = \"outer\"; while(flag) { val previous = replace(&held, \"next\"); break }; return held }\nfun entry(): Unit { val first = run(false); val second = run(true) }";
#[test]
fn selected_owner_actual_return_requires_product_authorization() {
    let source = UnitSource::analyze("owner-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, returns) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .is_ok_and(|s| s.starts_with("fun run("))
        })
        .unwrap();
    assert_eq!(run.blocks.iter().filter(|b| matches!(b.terminator.as_ref().map(|t| &t.kind), Some(TerminatorKind::Return { values }) if values.len() == 1)).count(), 1);
    let [event] = returns.as_slice() else {
        panic!("one actual explicit Return")
    };
    assert!(event.1.is_some());
    assert_eq!(
        event.2, event.1,
        "actual Return must consume the P/Q payload authorization"
    );
}
