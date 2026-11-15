//! Real sealed tail-break domain and complete caller certificate, no synthetic mutation.
use super::*;
use crate::ssa::{model::EntityId, refine::arm_loop_receipt, unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
#[test]
fn tail_break_complete_caller_certifies_only_sealed_exit_domain() {
    for signature in [
        "source:String,flag:Boolean,choose:Boolean",
        "source:String,choose:Boolean,flag:Boolean",
        "flag:Boolean,source:String,choose:Boolean",
        "flag:Boolean,choose:Boolean,source:String",
        "choose:Boolean,source:String,flag:Boolean",
        "choose:Boolean,flag:Boolean,source:String",
    ] {
        check(signature, "", "", true);
    }
}
#[test]
fn tail_break_complete_caller_keeps_surrounding_work_and_unused_roles_outside() {
    for (signature, prefix, suffix) in [
        (
            "source:String,flag:Boolean,choose:Boolean",
            "consume(source);other(source);",
            "",
        ),
        (
            "source:String,flag:Boolean,choose:Boolean",
            "",
            ";consume(source);other(source)",
        ),
        (
            "source:String,flag:Boolean,choose:Boolean,extra:Boolean",
            "",
            "",
        ),
        (
            "source:String,own flag:Boolean,choose:Boolean,extra:Boolean",
            "",
            "",
        ),
    ] {
        check(signature, prefix, suffix, false);
    }
}
pub(super) fn check(signature: &str, prefix: &str, suffix: &str, certified: bool) {
    check_roles(signature, prefix, suffix, certified, false);
}
pub(super) fn check_roles(
    signature: &str,
    prefix: &str,
    suffix: &str,
    certified: bool,
    same_flag: bool,
) {
    for swapped in [false, true] {
        for reverse in [false, true] {
            let mut sources = SourceMap::new();
            let (p, provider) = unit_lower_test_support::parsed(
                &mut sources,
                "arm-audit-provider.ko",
                "fun view(source:String):borrow String from source=source\nfun next(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}\nfun after(item:String):Unit {}\nfun before(item:String):Unit {}",
            );
            let (outer, inner) = if same_flag {
                ("flag", "flag")
            } else if swapped {
                ("choose", "flag")
            } else {
                ("flag", "choose")
            };
            let text = format!(
                "fun run({signature}):Unit {{{prefix}while({outer}){{if({inner}){{borrow val a=view(source);consume(a)}}else{{borrow val b=next(source);other(b);consume(b)}};break}}{suffix}}}"
            );
            let (c, caller) =
                unit_lower_test_support::parsed(&mut sources, "arm-audit-caller.ko", &text);
            let mut inputs = [
                SourceUnitInput::new("root", "arm-audit-provider.ko", p, &provider),
                SourceUnitInput::new("root", "arm-audit-caller.ko", c, &caller),
            ];
            if reverse {
                inputs.reverse();
            }
            let (environment, types) = standard_environments();
            let (names, typed, owned) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            assert!(owned.ownership().no_carry_branches().is_empty());
            let facts = owned.ownership().no_carry_break_branches();
            assert_eq!(facts.len(), 1);
            let fact = &facts[0];
            assert!(owned.ownership().owns_no_carry_break_branch(fact));
            assert!(fact.same_identity(&fact.clone()));
            let pair = fact.pair();
            assert_eq!(fact.exit_target(), pair.arms()[0].loop_statement());
            let break_span = caller
                .ast()
                .expressions()
                .get(fact.break_expression().expression())
                .unwrap()
                .span();
            assert_eq!(sources.slice(break_span).unwrap(), "break");
            assert_eq!(pair.arms()[0].consumers().len(), 1);
            assert_eq!(pair.arms()[1].consumers().len(), 2);
            assert_ne!(pair.arms()[0].binding(), pair.arms()[1].binding());
            assert_eq!(pair.arms()[0].source(), pair.arms()[1].source());
            let declaration = |name| {
                names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .find(|d| d.name() == name)
                    .unwrap()
                    .id()
            };
            for _ in 0..2 {
                let ((result, audits), receipts, trace) = arm_loop_receipt::observe(|| {
                    observe(|| {
                        unit_lower::lower_scalar_unit_with_entry(
                            &sources,
                            &inputs,
                            &names,
                            &types,
                            &typed,
                            &owned,
                            declaration("run"),
                        )
                    })
                });
                let (program, entry) = result.expect("original ordinary arm route");
                crate::ssa::verify::verify_program(&program).unwrap();
                assert_eq!(receipts.len(), 1);
                assert_eq!(trace.recorded, 1);
                assert_eq!(trace.before_verifier, 1);
                assert!(!trace.rolled_back);
                assert_eq!(trace.pending_after, 0);
                let function = program
                    .modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .find(|f| f.id == entry)
                    .unwrap();
                assert_eq!(receipts[0].caller, entry);
                receipts[0].validate(function).unwrap();
                let calls: Vec<_> = function
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                    .collect();
                assert_eq!(calls.len(), 2);
                let results: Vec<_> = calls
                    .iter()
                    .map(|i| {
                        assert_eq!(i.results.len(), 1);
                        i.results[0]
                    })
                    .collect();
                assert_ne!(results[0], results[1]);
                for result in results {
                    assert_eq!(function.instructions.iter().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==result)).count(),1);
                }
                assert_eq!(audits.len(), 1);
                for name in ["view", "next"] {
                    assert_eq!(
                        audits[0]
                            .borrow_edges
                            .contains(&(declaration("run"), declaration(name))),
                        certified
                    );
                }
                if !certified {
                    assert!(
                        audits[0]
                            .unsupported
                            .iter()
                            .any(|g| g == "tail-break arm-loop full-caller evidence gap")
                    );
                    continue;
                }
                assert!(
                    !audits[0]
                        .unsupported
                        .iter()
                        .any(|g| g.starts_with("tail-break arm-loop")
                            || g.starts_with("unclassified operation: BorrowCall"))
                );
                let breaks: Vec<_> = function
                    .blocks
                    .iter()
                    .filter(|b| {
                        b.terminator.as_ref().is_some_and(|t| {
                            t.origin == crate::ssa::model::Origin::Source(break_span)
                        })
                    })
                    .collect();
                assert_eq!(breaks.len(), 1);
                let TerminatorKind::Branch(edge) = &breaks[0].terminator.as_ref().unwrap().kind
                else {
                    panic!("real tail break edge");
                };
                let exit = function.block(edge.target).unwrap();
                assert!(
                    matches!(&exit.terminator.as_ref().unwrap().kind, TerminatorKind::Return { values } if values.is_empty())
                );
                assert_eq!(exit.parameters.len(), usize::from(!suffix.is_empty()));
                assert_eq!(exit.instructions.len(), usize::from(!suffix.is_empty()));
                assert_eq!(function.blocks.len(), 8);
                assert_eq!(
                    function.instructions.len(),
                    9 + usize::from(!suffix.is_empty()) + usize::from(!prefix.is_empty())
                        - signature
                            .split(',')
                            .filter(|p| p.starts_with("own "))
                            .count()
                );
                assert!(breaks[0].instructions.is_empty());
                assert_eq!(edge.arguments.len(), usize::from(!suffix.is_empty()));
                let entry_block = function.block(function.entry_block().unwrap()).unwrap();
                assert_eq!(
                    entry_block.instructions.len(),
                    usize::from(!prefix.is_empty())
                );
                if !prefix.is_empty() {
                    let call = function.instruction(entry_block.instructions[0]).unwrap();
                    let name = prefix.split('(').next().unwrap();
                    let declaration = names
                        .names()
                        .index()
                        .declarations()
                        .iter()
                        .find(|d| d.name() == name)
                        .unwrap();
                    let span = provider
                        .ast()
                        .items()
                        .get(declaration.root())
                        .unwrap()
                        .span();
                    let targets: Vec<_> = program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .filter(|f| f.origin == crate::ssa::model::Origin::Source(span))
                        .collect();
                    let [target] = targets.as_slice() else {
                        panic!("original prefix declaration");
                    };
                    let source_role = signature
                        .split(',')
                        .position(|p| p == "source:String")
                        .unwrap();
                    assert_eq!(
                        call.operation,
                        Operation::DirectCall {
                            callee: target.id,
                            receiver: None,
                            arguments: vec![entry_block.parameters[source_role]]
                        }
                    );
                    assert!(call.results.is_empty());
                    let call_text = prefix.trim_end_matches(';');
                    let start = text.find(call_text).unwrap();
                    assert_eq!(
                        call.origin,
                        crate::ssa::model::Origin::Source(
                            sources.span(c, start, start + call_text.len()).unwrap()
                        )
                    );
                    if !suffix.is_empty() {
                        assert_ne!(call.id, exit.instructions[0]);
                        assert_ne!(
                            call.origin,
                            function.instruction(exit.instructions[0]).unwrap().origin
                        );
                    }
                }
                let mut roles: BTreeMap<_, _> = entry_block
                    .parameters
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(i, e)| (e, i))
                    .collect();
                for _ in 0..function.blocks.len() {
                    for block in &function.blocks {
                        let edges = match &block.terminator.as_ref().unwrap().kind {
                            TerminatorKind::Branch(e) => vec![e],
                            TerminatorKind::Conditional {
                                when_true,
                                when_false,
                                ..
                            } => vec![when_true, when_false],
                            _ => vec![],
                        };
                        for edge in edges {
                            for (argument, parameter) in edge
                                .arguments
                                .iter()
                                .zip(&function.block(edge.target).unwrap().parameters)
                            {
                                if let Some(role) = roles.get(argument).copied()
                                    && let Some(previous) = roles.insert(*parameter, role)
                                {
                                    assert_eq!(previous, role);
                                }
                            }
                        }
                    }
                }
                if !suffix.is_empty() {
                    let source_role = signature
                        .split(',')
                        .position(|p| p == "source:String")
                        .unwrap();
                    assert_eq!(roles.get(&exit.parameters[0]), Some(&source_role));
                    let source_entity = exit.parameters[0];
                    let call = function.instruction(exit.instructions[0]).unwrap();
                    let declaration = names
                        .names()
                        .index()
                        .declarations()
                        .iter()
                        .find(|d| d.name() == "after")
                        .unwrap();
                    let span = provider
                        .ast()
                        .items()
                        .get(declaration.root())
                        .unwrap()
                        .span();
                    let targets: Vec<_> = program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .filter(|f| f.origin == crate::ssa::model::Origin::Source(span))
                        .collect();
                    let [target] = targets.as_slice() else {
                        panic!("original suffix declaration");
                    };
                    assert_eq!(
                        call.operation,
                        Operation::DirectCall {
                            callee: target.id,
                            receiver: None,
                            arguments: vec![source_entity]
                        }
                    );
                    assert!(call.results.is_empty());
                    let start = text.rfind("after(source)").unwrap();
                    assert_eq!(
                        call.origin,
                        crate::ssa::model::Origin::Source(
                            sources
                                .span(c, start, start + "after(source)".len())
                                .unwrap()
                        )
                    );
                    let predecessors: Vec<_> = function
                        .blocks
                        .iter()
                        .filter_map(|b| match &b.terminator.as_ref().unwrap().kind {
                            TerminatorKind::Branch(e) if e.target == exit.id => Some((b, e)),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(predecessors.len(), 2);
                    for (block, incoming) in predecessors {
                        assert!(block.instructions.is_empty());
                        assert_eq!(incoming.arguments.len(), 1);
                        assert_eq!(roles.get(&incoming.arguments[0]), Some(&source_role));
                    }
                }
                let mut read_results = BTreeSet::new();
                let TerminatorKind::Branch(to_header) =
                    &entry_block.terminator.as_ref().unwrap().kind
                else {
                    panic!("entry header");
                };
                let header = function.block(to_header.target).unwrap();
                let TerminatorKind::Conditional { when_true, .. } =
                    &header.terminator.as_ref().unwrap().kind
                else {
                    panic!("outer condition");
                };
                let split = function.block(when_true.target).unwrap();
                for ((keyword, name), block) in [("while", outer), ("if", inner)]
                    .into_iter()
                    .zip([header, split])
                {
                    let start =
                        text.find(&format!("{keyword}({name})")).unwrap() + keyword.len() + 1;
                    let span = sources.span(c, start, start + name.len()).unwrap();
                    let (role, parameter) = signature
                        .split(',')
                        .enumerate()
                        .find(|(_, p)| {
                            p.trim_start_matches("own ")
                                .starts_with(&format!("{name}:"))
                        })
                        .unwrap();
                    let TerminatorKind::Conditional { condition, .. } =
                        block.terminator.as_ref().unwrap().kind
                    else {
                        panic!("original conditional");
                    };
                    if parameter.starts_with("own ") {
                        assert!(block.instructions.is_empty());
                        assert_eq!(roles.get(&EntityId::Value(condition)), Some(&role));
                    } else {
                        let [instruction] = block.instructions.as_slice() else {
                            panic!("one original condition read");
                        };
                        let read = function.instruction(*instruction).unwrap();
                        assert_eq!(read.origin, crate::ssa::model::Origin::Source(span));
                        let Operation::Read {
                            source: crate::ssa::model::PlaceAccess::Loan(flag),
                        } = read.operation
                        else {
                            panic!("shared flag read");
                        };
                        assert_eq!(roles.get(&EntityId::Loan(flag)), Some(&role));
                        assert_eq!(read.results, [EntityId::Value(condition)]);
                        assert!(read_results.insert(condition));
                    }
                }
                for arm in pair.arms() {
                    let initializer_span = caller
                        .ast()
                        .expressions()
                        .get(arm.initializer().expression())
                        .unwrap()
                        .span();
                    let initializers: Vec<_> = calls
                        .iter()
                        .filter(|i| i.origin == crate::ssa::model::Origin::Source(initializer_span))
                        .collect();
                    let [initializer] = initializers.as_slice() else {
                        panic!("original arm initializer");
                    };
                    let result = initializer.results[0];
                    let block = function.block(initializer.block).unwrap();
                    assert_eq!(block.instructions.len(), arm.consumers().len() + 2);
                    assert_eq!(block.instructions[0], initializer.id);
                    for (index, (_, expression, _)) in arm.consumers().iter().enumerate() {
                        let call = function.instruction(block.instructions[index + 1]).unwrap();
                        assert_eq!(
                            call.origin,
                            crate::ssa::model::Origin::Source(
                                caller
                                    .ast()
                                    .expressions()
                                    .get(expression.expression())
                                    .unwrap()
                                    .span()
                            )
                        );
                        let Operation::DirectCall {
                            arguments,
                            receiver: None,
                            ..
                        } = &call.operation
                        else {
                            panic!("original consumer");
                        };
                        assert_eq!(arguments, &[result]);
                        assert!(call.results.is_empty());
                    }
                    assert!(
                        matches!(function.instruction(*block.instructions.last().unwrap()).unwrap().operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan)==result)
                    );
                    let TerminatorKind::Branch(arm_edge) = &block.terminator.as_ref().unwrap().kind
                    else {
                        panic!("arm closes before break");
                    };
                    assert_eq!(arm_edge.target, breaks[0].id);
                    assert!(!arm_edge.arguments.contains(&result));
                }
            }
        }
    }
}
