//! Ordinary finite original-source probe for a replaced owner crossing a normal backedge.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    var again = flag
    while(again) { val previous = replace(&held, "next"); again = false }
    return held
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn normal_backedge_original_source_probe() {
    use crate::ssa::model::{Operation, TerminatorKind};
    let source = UnitSource::analyze("owner-normal-backedge.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (program, entry) = source.lower_entry(source.entry()).unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    eprintln!("OWNERSHIP {:#?}", source.ownership());
    eprintln!("SSA {}", crate::ssa::render_program(&program));
    let run = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .unwrap()
                .starts_with("fun run(")
        })
        .unwrap();
    let replacements = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    let [replacement] = replacements.as_slice() else {
        panic!("one real body Replace");
    };
    let body = run.block(replacement.block).unwrap();
    assert!(matches!(
        body.terminator.as_ref().unwrap().kind,
        TerminatorKind::Branch { .. }
    ));
    assert_eq!(
        run.blocks
            .iter()
            .filter(|b| matches!(
                b.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ))
            .count(),
        1
    );
}
