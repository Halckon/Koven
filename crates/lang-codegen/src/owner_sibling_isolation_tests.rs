//! Ordinary legal neighbors must not share or erase parent/child owner authority.
use super::{OwnedProducts, UnitSource};
const MARKERS: [&str; 3] = [
    "parent-initial-owner-base-ssa-complete",
    "parent-generation-composition-ssa-complete",
    "child-cycle-ssa-complete",
];
fn function(name: &str, complete: bool) -> String {
    let initial = if complete { "seed" } else { "seed+\"extra\"" };
    format!(
        r#"fun {name}(outer:Int,inner:Int,own seed:String) {{
        var a={initial}; var b="{name}-b"; var n0=outer; var g0=n0>0;
        while(g0) {{ var n1=inner; var g1=n1>0;
            while(g1) {{ val x=replace(&a,"{name}-child"); val y=replace(&b,"{name}-child-b");
                n1=n1-1; g1=n1>0 }};
            val old=replace(&b,"{name}-parent"); n0=n0-1; g0=n0>0
        }}; println(a); println(b)
    }}"#
    )
}
fn analyze(definitions: &str, calls: &str, constant: bool) -> UnitSource {
    let text = format!(
        "{}\n{definitions}\nfun entry() {{ {calls} }}",
        if constant { "const val ONCE:Int=1" } else { "" }
    );
    let source = UnitSource::analyze("owner-sibling-isolation.ko", &text).unwrap();
    source.assert_frontend_clean();
    assert_eq!(
        matches!(source.products, OwnedProducts::Constant(..)),
        constant
    );
    source
}
fn receipts(source: &UnitSource) -> Vec<(String, usize)> {
    let (result, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, entry) = result.unwrap_or_else(|e| panic!("{e:?}; {actions:?}"));
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let mut selected = actions
        .into_iter()
        .filter(|(name, _)| MARKERS.contains(name))
        .map(|(name, count)| (name.to_owned(), count))
        .collect::<Vec<_>>();
    selected.sort();
    selected
}
fn mixed(reverse: bool, constant: bool) -> (UnitSource, Vec<(String, usize)>) {
    let full = function("full", true);
    let neighbor = function("neighbor", false);
    let once = if constant { "ONCE" } else { "1" };
    let full_calls = format!("full(0,0,\"F\"); full({once},2,\"F\");");
    let neighbor_calls = format!("neighbor(0,0,\"N\"); neighbor({once},2,\"N\");");
    let mut full_receipts = receipts(&analyze(&full, &full_calls, constant));
    assert_eq!(
        full_receipts
            .iter()
            .filter(|(name, _)| name == MARKERS[0])
            .count(),
        1
    );
    let neighbor_receipts = receipts(&analyze(&neighbor, &neighbor_calls, constant));
    assert!(
        !neighbor_receipts.iter().any(|(name, _)| name == MARKERS[0]),
        "legal nonliteral concat input retains its existing lack of initial-base authority"
    );
    full_receipts.extend(neighbor_receipts);
    full_receipts.sort();
    let definitions = if reverse {
        format!("{neighbor}\n{full}")
    } else {
        format!("{full}\n{neighbor}")
    };
    let calls = format!(
        "full(0,0,\"F\"); neighbor(0,0,\"N\"); neighbor({once},2,\"N\"); full({once},2,\"F\");"
    );
    (analyze(&definitions, &calls, constant), full_receipts)
}
#[test]
fn sibling_owner_authority_matches_independent_legal_functions() {
    for reverse in [false, true] {
        for constant in [false, true] {
            let (source, expected) = mixed(reverse, constant);
            assert_eq!(
                receipts(&source),
                expected,
                "the same Unit must preserve each callable's own capability without leakage"
            );
        }
    }
}
#[test]
fn sibling_owner_native_results_remain_callable_local() {
    for reverse in [false, true] {
        for constant in [false, true] {
            let (source, expected) = mixed(reverse, constant);
            assert_eq!(receipts(&source), expected);
            super::assert_native_output(&source,
                b"F\nfull-b\nNextra\nneighbor-b\nneighbor-child\nneighbor-parent\nfull-child\nfull-parent\n");
        }
    }
}
