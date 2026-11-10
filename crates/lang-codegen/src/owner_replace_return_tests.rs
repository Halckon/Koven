//! Ordinary finite suffix Replace probe through original admitted products.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    return replace(&held, "last")
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn suffix_replace_return_original_source_probe() {
    let source = UnitSource::analyze("suffix-replace-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    use lang_frontend::ownership_checking::UnitDropPoint;
    let suffix = "replace(&held, \"last\")";
    let drops = source
        .ownership()
        .drops()
        .iter()
        .filter(|drop| source.sources.slice(drop.value_origin()).unwrap() == suffix)
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one actual root cleanup");
    };
    let UnitDropPoint::CallReturn(call) = drop.point() else {
        panic!("R retires at CallReturn");
    };
    assert_eq!(
        source
            .sources
            .slice(
                source
                    .parsed
                    .ast()
                    .expressions()
                    .get(call.expression())
                    .unwrap()
                    .span()
            )
            .unwrap(),
        suffix
    );
    let (program, entry) = source.lower_entry(source.entry()).unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
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
    let exchanges = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    assert_eq!(exchanges.len(), 2);
    let exchange = exchanges
        .iter()
        .find(|i| source.sources.slice(i.origin.span()).unwrap() == suffix)
        .unwrap();
    let exit = run.block(exchange.block).unwrap();
    let Operation::RootReplace { replacement, .. } = exchange.operation else {
        unreachable!()
    };
    let literal = run
        .instructions
        .iter()
        .find(
            |i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"last"),
        )
        .unwrap();
    assert_eq!(literal.results, [EntityId::Value(replacement)]);
    let drops = run
        .instructions
        .iter()
        .filter(|i| i.block == exit.id && matches!(i.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one installed R drop at exit");
    };
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    assert_eq!(EntityId::Value(owner), exchange.results[0]);
    let TerminatorKind::Return { values } = &exit.terminator.as_ref().unwrap().kind else {
        panic!("old payload Return");
    };
    assert_eq!(values.len(), 1);
    assert_eq!(EntityId::Value(values[0]), exchange.results[1]);
    let position = |id| exit.instructions.iter().position(|i| *i == id).unwrap();
    assert!(position(exchange.id) < position(drop.id));
}
