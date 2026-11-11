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

#[test]
fn normal_header_final_drop_consumes_current_h_not_fixed_p_or_q() {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    let source = UnitSource::analyze("header-bind-ssa.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, drops) = crate::ssa::observe_owner_drops(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
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
    let TerminatorKind::Branch(initial) = &entry.terminator.as_ref().unwrap().kind else {
        panic!("entry")
    };
    let header = run.block(initial.target).unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("header")
    };
    let body = run.block(when_true.target).unwrap();
    let exit = run.block(when_false.target).unwrap();
    let replace = run
        .instructions
        .iter()
        .find(|i| i.block == body.id && matches!(i.operation, Operation::RootReplace { .. }))
        .unwrap();
    let Operation::RootReplace { owner, .. } = replace.operation else {
        unreachable!()
    };
    let body_index = body
        .parameters
        .iter()
        .position(|v| *v == EntityId::Value(owner))
        .unwrap();
    let h = when_true.arguments[body_index];
    let hi = header.parameters.iter().position(|v| *v == h).unwrap();
    let p = run
        .instructions
        .iter()
        .find(|i| matches!(&i.operation,Operation::StringLiteral{bytes, ..} if bytes==b"outer"))
        .unwrap();
    assert_eq!(initial.arguments[hi], p.results[0]);
    let TerminatorKind::Conditional {
        when_true: abort,
        when_false: success,
        ..
    } = &body.terminator.as_ref().unwrap().kind
    else {
        panic!("checked increment")
    };
    assert!(matches!(
        run.block(abort.target)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind,
        TerminatorKind::Abort
    ));
    let back = run.block(success.target).unwrap();
    let TerminatorKind::Branch(backedge) = &back.terminator.as_ref().unwrap().kind else {
        panic!("normal edge")
    };
    assert_eq!(backedge.target, header.id);
    let qi = success
        .arguments
        .iter()
        .position(|v| *v == replace.results[0])
        .unwrap();
    assert_eq!(backedge.arguments[hi], back.parameters[qi]);
    let final_drops = run
        .instructions
        .iter()
        .filter(|i| i.block == exit.id && matches!(i.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [final_drop] = final_drops.as_slice() else {
        panic!("one actual final Drop")
    };
    let Operation::Drop { owner: final_owner } = final_drop.operation else {
        unreachable!()
    };
    let exit_index = exit
        .parameters
        .iter()
        .position(|v| *v == EntityId::Value(final_owner))
        .unwrap();
    assert_eq!(when_false.arguments[exit_index], h);
    assert_ne!(EntityId::Value(final_owner), p.results[0]);
    assert_ne!(EntityId::Value(final_owner), replace.results[0]);
    let incoming = |target| {
        run.blocks
            .iter()
            .filter(|b| {
                b.terminator
                    .as_ref()
                    .is_some_and(|t| t.kind.targets().contains(&target))
            })
            .map(|b| b.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(incoming(header.id), [entry.id, back.id]);
    assert_eq!(incoming(body.id), [header.id]);
    assert_eq!(incoming(back.id), [body.id]);
    assert_eq!(incoming(exit.id), [header.id]);
    let authorized = drops
        .iter()
        .filter_map(|d| d.3.map(|r| (d, r)))
        .collect::<Vec<_>>();
    let [(physical, (selected, bound, index))] = authorized.as_slice() else {
        panic!("one final BindDrop receipt")
    };
    assert_eq!((*bound, *index), (physical.2, physical.1));
    assert_ne!(selected, bound);
    assert!(
        matches!(&exit.terminator.as_ref().unwrap().kind,TerminatorKind::Return{values} if values.is_empty())
    );
}

#[test]
fn normal_header_bind_basic_const_finite_native() {
    let printed=SOURCE.replace("run(0); run(1); run(2); run(7)",
        "run(0); println(\"zero\"); run(1); println(\"one\"); run(2); println(\"two\"); run(7); println(\"seven\")");
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val SEVEN: Int = 7\n{}",
                printed.replace("run(7)", "run(SEVEN)")
            )
        } else {
            printed.clone()
        };
        let source = UnitSource::analyze("header-bind-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        let (program, entry) = source.lower_entry(source.entry()).unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        super::assert_native_output(&source, b"zero\none\ntwo\nseven\n");
    }
}

#[test]
fn ci_only_header_bind_rejects_missing_or_duplicate_drop_completion() {
    for fault in [
        crate::ssa::OwnerFault::MissingHeaderBindDrop,
        crate::ssa::OwnerFault::DuplicateHeaderBindDrop,
    ] {
        let source = UnitSource::analyze("header-bind-fault.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
        assert!(result.is_err(), "final Drop must complete exactly once");
    }
}
