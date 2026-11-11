//! Normal header final local consumption must carry actual Drop authorization.
use super::{OwnedProducts, UnitSource};
use lang_frontend::ownership_checking::UnitDropTarget;

const SOURCE: &str = r#"fun run(limit: Int): Unit {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
    }
    val retired = held
}
fun entry(): Unit { run(0); run(1); run(2); run(7) }
"#;

#[test]
fn normal_header_final_binding_requires_actual_drop_authority() {
    let source =
        UnitSource::analyze("owner-header-bind.ko", SOURCE).expect("ordinary source analyzes");
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let symbols = source
        .names()
        .names()
        .source_units()
        .iter()
        .flat_map(|unit| {
            unit.resolution()
                .symbols()
                .iter()
                .filter(|s| s.name() == "retired")
                .map(move |s| (unit.source_unit(), s.id()))
        })
        .collect::<Vec<_>>();
    let [retired] = symbols.as_slice() else {
        panic!("one original retired declaration")
    };
    let OwnedProducts::Basic(_, owned) = &source.products else {
        unreachable!()
    };
    let facts = owned
        .ownership()
        .drops()
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            matches!(d.target(), UnitDropTarget::Named(target)
            if (target.source_unit(), target.symbol()) == *retired)
        })
        .collect::<Vec<_>>();
    let [(index, fact)] = facts.as_slice() else {
        panic!("one actual retired Drop fact")
    };
    let (lowered, drops) = crate::ssa::observe_owner_drops(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.expect("real header final binding lowers");
    crate::ssa::verify::verify_program(&program).expect("real SSA verifies");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("verified SSA renders LLVM");
    let observed = drops
        .iter()
        .filter(|d| d.0 == fact.target() && d.1 == *index)
        .collect::<Vec<_>>();
    let [actual] = observed.as_slice() else {
        panic!("one exact emitted retired Drop")
    };
    // This observer proves actual consumption authority. Header provenance is a
    // separate SSA/transport proof, not inferred merely from an emitted Drop.
    let (selected, bound, authorized_index) = actual
        .3
        .expect("header final binding lacks actual Drop authority");
    assert_eq!(
        bound, actual.2,
        "authorized bound slot is the physical Drop operand"
    );
    assert_eq!(
        authorized_index, *index,
        "authorization uses the original Drop fact"
    );
    assert_ne!(
        selected, bound,
        "selected owner is transported to a distinct local binding"
    );
}
