//! Ordinary original-source owner handoff; no damaged inputs or resource counters.
use super::UnitSource;
use crate::ssa::{model::Operation, verify::verify_program};

const SOURCE: &str = "fun sink(own item: String): Unit {}\nfun run(flag: Boolean) { var held = \"outer\"; while(flag) { val previous = replace(&held, \"next\"); break }; sink(held) }\nfun entry(): Unit { run(false); run(true) }";

#[test]
fn selected_owner_actual_call_requires_product_authorization() {
    let source = UnitSource::analyze("owner-own-call.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, calls) = crate::ssa::observe_owner_calls(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let functions = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .collect::<Vec<_>>();
    let sink = functions
        .iter()
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .is_ok_and(|s| s.starts_with("fun sink("))
        })
        .unwrap();
    let run = functions
        .iter()
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .is_ok_and(|s| s.starts_with("fun run("))
        })
        .unwrap();
    let physical = run
        .instructions
        .iter()
        .filter(
            |i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == sink.id()),
        )
        .collect::<Vec<_>>();
    assert_eq!(
        physical.len(),
        1,
        "the existing emitter makes the actual call"
    );
    let events = calls
        .iter()
        .filter(|c| c.1 == sink.id())
        .collect::<Vec<_>>();
    let [event] = events.as_slice() else {
        panic!("one actual sink call")
    };
    assert_eq!(event.2.len(), 1);
    assert_eq!(
        event.3,
        Some(event.2[0]),
        "actual own call must consume the selected P/Q product authorization"
    );
}
