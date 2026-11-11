//! Runtime split thresholds must keep mixed header and exit authority distinct.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int, stop: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
        if(index < stop) { continue } else { break }
    }
    return held
}
fun entry(): Unit { val a = run(0, 2); val b = run(1, 2); val c = run(7, 2); val d = run(2, 7) }
"#;

#[test]
fn parameter_mixed_requires_actual_exit_return_authority() {
    let source = UnitSource::analyze("owner-parameter-mixed.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [receipt] = receipts.as_slice() else {
        panic!("one owned Return");
    };
    assert!(
        receipt.2.is_some(),
        "runtime mixed threshold lacks real Return authority"
    );
    assert_eq!(receipt.1, receipt.2);
}

#[test]
fn distinct_parameters_feed_header_and_mixed_split_comparisons() {
    use crate::ssa::model::{
        ComparisonOperator, EntityId, EntityType, Function, LoanKind, Operation, PlaceAccess,
        SsaTypeKind, TerminatorKind,
    };
    // Follow block parameters and exact single-result reads of parameter loans.
    // No arithmetic, copies, place reads, or calls can manufacture RHS identity.
    fn roots(function: &Function, value: EntityId) -> Vec<EntityId> {
        let entry = function.blocks.first().unwrap();
        let mut pending = vec![value];
        let mut visited = Vec::new();
        let mut result = Vec::new();
        while let Some(value) = pending.pop() {
            if visited.contains(&value) {
                continue;
            }
            visited.push(value);
            if entry.parameters.contains(&value) {
                result.push(value);
                continue;
            }
            if matches!(value, EntityId::Value(_)) {
                let definitions = function
                    .instructions
                    .iter()
                    .filter(|instruction| instruction.results.contains(&value))
                    .collect::<Vec<_>>();
                if !definitions.is_empty() {
                    let [definition] = definitions.as_slice() else {
                        panic!("parameter read must have exactly one definition");
                    };
                    assert_eq!(definition.results, [value]);
                    let Operation::Read {
                        source: PlaceAccess::Loan(loan),
                    } = definition.operation
                    else {
                        panic!("only a direct loan read may forward parameter identity");
                    };
                    pending.push(EntityId::Loan(loan));
                    continue;
                }
            }
            let (block, position) = function
                .blocks
                .iter()
                .find_map(|block| {
                    block
                        .parameters
                        .iter()
                        .position(|p| *p == value)
                        .map(|i| (block, i))
                })
                .expect("parameter RHS must be forwarded from entry, not synthesized");
            let mut incoming = 0;
            for predecessor in &function.blocks {
                let Some(term) = &predecessor.terminator else {
                    continue;
                };
                let edges = match &term.kind {
                    TerminatorKind::Branch(edge) => vec![edge],
                    TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } => vec![when_true, when_false],
                    _ => Vec::new(),
                };
                for edge in edges.into_iter().filter(|edge| edge.target == block.id) {
                    incoming += 1;
                    pending.push(edge.arguments[position]);
                }
            }
            assert!(incoming > 0);
        }
        result
    }
    let source = UnitSource::analyze("parameter-mixed-ssa.ko", SOURCE).unwrap();
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
    let [limit, stop] = run.blocks.first().unwrap().parameters.as_slice() else {
        panic!("exactly two independent runtime parameters");
    };
    assert_ne!(limit, stop);
    for parameter in [*limit, *stop] {
        assert!(matches!(parameter, EntityId::Loan(_)));
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = run.entity(parameter).unwrap().ty
        else {
            panic!("each runtime parameter must be a shared Int loan");
        };
        assert_eq!(
            program.module(target.module()).unwrap().types[target.index()],
            SsaTypeKind::Integer {
                bits: 32,
                signed: true
            }
        );
    }

    let compare = |text: &str| {
        run.instructions
            .iter()
            .find(|i| {
                matches!(i.operation, Operation::Compare { .. })
                    && source.sources.slice(i.origin.span()).unwrap() == text
            })
            .unwrap()
    };
    let header_comparison = compare("index < limit");
    let split_comparison = compare("index < stop");
    for (instruction, parameter) in [(header_comparison, *limit), (split_comparison, *stop)] {
        let Operation::Compare {
            operator, right, ..
        } = instruction.operation
        else {
            unreachable!()
        };
        assert_eq!(operator, ComparisonOperator::LessThan);
        assert_eq!(roots(run, EntityId::Value(right)), [parameter]);
    }
    let header = run.block(header_comparison.block).unwrap();
    let split = run.block(split_comparison.block).unwrap();
    let TerminatorKind::Conditional {
        condition: header_condition,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("header conditional")
    };
    assert_eq!(
        header_comparison.results,
        [EntityId::Value(*header_condition)]
    );
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &split.terminator.as_ref().unwrap().kind
    else {
        panic!("runtime split conditional")
    };
    assert_eq!(split_comparison.results, [EntityId::Value(*condition)]);
    let continue_term = run
        .block(when_true.target)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap();
    let break_term = run
        .block(when_false.target)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap();
    assert_eq!(
        source.sources.slice(continue_term.origin.span()).unwrap(),
        "continue"
    );
    assert_eq!(
        source.sources.slice(break_term.origin.span()).unwrap(),
        "break"
    );
    let TerminatorKind::Branch(continued) = &continue_term.kind else {
        panic!("real Continue")
    };
    let TerminatorKind::Branch(broken) = &break_term.kind else {
        panic!("real Break")
    };
    assert_eq!(continued.target, header.id);
    assert_ne!(continued.target, broken.target);
    assert!(matches!(
        run.block(broken.target)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind,
        TerminatorKind::Return { .. }
    ));
}

#[test]
fn parameter_mixed_basic_and_const_finite_native() {
    let original_entry =
        "val a = run(0, 2); val b = run(1, 2); val c = run(7, 2); val d = run(2, 7)";
    let printed = SOURCE.replace(original_entry,
        "val a = run(0, 2); println(a); val b = run(1, 2); println(b); val c = run(7, 2); println(c); val d = run(2, 7); println(d); val e = run(2, 0); println(e); val f = run(2, -1); println(f)");
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val SEVEN: Int = 7\n{}",
                printed.replace("run(7, 2)", "run(SEVEN, 2)")
            )
        } else {
            printed.clone()
        };
        let source = UnitSource::analyze("parameter-mixed-native.ko", &text).unwrap();
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
        let [receipt] = receipts.as_slice() else {
            panic!("one parameter mixed Return")
        };
        assert!(receipt.2.is_some());
        assert_eq!(receipt.1, receipt.2);
        super::assert_native_output(&source, b"outer\nnext\nnext\nnext\nnext\nnext\n");
    }
}
