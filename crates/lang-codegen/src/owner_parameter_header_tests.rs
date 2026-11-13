//! Owned parameter entry must retain its identity through a changing normal header.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int, own incoming: String): String {
    var held = incoming
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
    }
    return held
}
fun entry(): Unit { val a = run(0, "outer"); val b = run(1, "outer"); val c = run(2, "outer"); val d = run(7, "outer") }
"#;

#[test]
fn owned_parameter_normal_header_requires_actual_return_authority() {
    let source = UnitSource::analyze("owner-parameter-header.ko", SOURCE)
        .expect("ordinary owned parameter source must analyze");
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.expect("owned parameter normal loop must lower");
    crate::ssa::verify::verify_program(&program).expect("real SSA must verify");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("verified SSA must render LLVM");
    let [receipt] = receipts.as_slice() else {
        panic!("exactly one actual owned Return, got {receipts:?}");
    };
    assert!(receipt.1.is_some(), "Return must have an actual payload");
    assert!(
        receipt.2.is_some(),
        "owned parameter header lacks actual Return authority"
    );
    assert_eq!(
        receipt.1, receipt.2,
        "authority must match the actual payload"
    );
}

#[test]
fn parameter_entry_is_the_actual_header_input_not_a_literal() {
    use crate::ssa::model::{EntityId, EntityType, Operation, TerminatorKind};
    let source = UnitSource::analyze("parameter-header-ssa.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
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
    let entry = run.blocks.first().unwrap();
    let [_, incoming] = entry.parameters.as_slice() else {
        panic!("two ABI parameters")
    };
    assert!(matches!(incoming, EntityId::Value(_)));
    assert!(matches!(
        run.entity(*incoming).unwrap().ty,
        EntityType::Value(_)
    ));
    assert!(
        !run.instructions
            .iter()
            .any(|i| i.results.contains(incoming)),
        "P is an ABI value, not an instruction producer"
    );
    let literals = run
        .instructions
        .iter()
        .filter_map(|i| match &i.operation {
            Operation::StringLiteral { bytes, .. } => Some(bytes.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(literals, [b"next".as_slice()], "only Q is a literal");
    let replace = run
        .instructions
        .iter()
        .find(|i| matches!(i.operation, Operation::RootReplace { .. }))
        .unwrap();
    let Operation::RootReplace { owner, .. } = replace.operation else {
        unreachable!()
    };
    let body = run.block(replace.block).unwrap();
    let root_index = body
        .parameters
        .iter()
        .position(|v| *v == EntityId::Value(owner))
        .unwrap();
    let TerminatorKind::Branch(initial_edge) = &entry.terminator.as_ref().unwrap().kind else {
        panic!("entry to header")
    };
    let header = run.block(initial_edge.target).unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("header")
    };
    assert_eq!(when_true.target, body.id);
    let h = when_true.arguments[root_index];
    let header_index = header.parameters.iter().position(|v| *v == h).unwrap();
    assert_eq!(initial_edge.arguments[header_index], *incoming);
    let TerminatorKind::Conditional {
        when_false: success,
        ..
    } = &body.terminator.as_ref().unwrap().kind
    else {
        panic!("checked increment")
    };
    let back = run.block(success.target).unwrap();
    let TerminatorKind::Branch(backedge) = &back.terminator.as_ref().unwrap().kind else {
        panic!("normal backedge")
    };
    assert_eq!(backedge.target, header.id);
    let q_index = success
        .arguments
        .iter()
        .position(|v| *v == replace.results[0])
        .unwrap();
    assert_eq!(backedge.arguments[header_index], back.parameters[q_index]);
    let sources = run
        .blocks
        .iter()
        .filter(|b| {
            b.terminator
                .as_ref()
                .is_some_and(|t| t.kind.targets().contains(&header.id))
        })
        .map(|b| b.id)
        .collect::<Vec<_>>();
    assert_eq!(sources, [entry.id, back.id]);
    let exit = run.block(when_false.target).unwrap();
    let TerminatorKind::Return { values } = &exit.terminator.as_ref().unwrap().kind else {
        panic!("current H Return")
    };
    let position = exit
        .parameters
        .iter()
        .position(|v| *v == EntityId::Value(values[0]))
        .unwrap();
    assert_eq!(when_false.arguments[position], h);
}

#[test]
fn parameter_header_basic_const_finite_native_preserves_caller_input() {
    let printed=SOURCE.replace("val a = run(0, \"outer\"); val b = run(1, \"outer\"); val c = run(2, \"outer\"); val d = run(7, \"outer\")",
        "val a = run(0, \"zero-a\"); println(a); val b = run(1, \"one\"); println(b); val c = run(2, \"two\"); println(c); val d = run(7, \"seven\"); println(d); val e = run(0, \"zero-b\"); println(e)");
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val SEVEN: Int = 7\n{}",
                printed.replace("run(7,", "run(SEVEN,")
            )
        } else {
            printed.clone()
        };
        let source = UnitSource::analyze("parameter-header-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        let (lowered, receipts) =
            crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let [r] = receipts.as_slice() else {
            panic!("one Return")
        };
        assert!(r.2.is_some());
        assert_eq!(r.1, r.2);
        super::assert_native_output(&source, b"zero-a\nnext\nnext\nnext\nzero-b\n");
    }
}

#[test]
fn ci_only_parameter_header_rejects_literal_forged_as_entry() {
    let source = UnitSource::analyze("parameter-header-forged.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let result =
        crate::ssa::with_owner_fault(crate::ssa::OwnerFault::ForgedHeaderParameterLiteral, || {
            source.lower_entry(source.entry())
        });
    assert!(
        result.is_err(),
        "entry ABI producer cannot be redefined by a literal"
    );
}
