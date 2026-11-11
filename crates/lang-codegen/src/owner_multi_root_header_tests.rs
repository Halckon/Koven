//! Reuse actual SSA construction/LLVM phis, never manufacture new owner authority.
use super::UnitSource;
use crate::ssa::model::{EntityId, Operation, TerminatorKind};
use lang_frontend::ownership_checking::{UnitOwnerHeaderSourceV1, UnitOwnerOrigin};

fn source(roots: usize, reverse: bool, live: usize) -> String {
    let prefix = (0..roots)
        .map(|i| format!("var r{i} = \"entry{i}\";"))
        .collect::<String>();
    let mut order: Vec<_> = (0..roots).collect();
    if reverse {
        order.reverse();
    }
    let body = order
        .into_iter()
        .map(|i| format!("val old{i} = replace(&r{i}, \"body{i}\");"))
        .collect::<String>();
    let suffix = (0..live)
        .rev()
        .map(|i| format!("val end{i} = r{i};"))
        .collect::<String>();
    format!(
        "fun run(flag: Boolean) {{ {prefix} while(flag) {{ {body} }}; {suffix} }}\nfun entry() {{ run(false) }}"
    )
}

#[test]
fn multi_root_header_original_source_phi_and_cleanup_matrix() {
    for roots in [2, 3] {
        for reverse in [false, true] {
            for live in [0, 1, roots] {
                let source =
                    UnitSource::analyze("multi-root-header.ko", &source(roots, reverse, live))
                        .unwrap();
                source.assert_frontend_clean();
                let boundary = source
                    .ownership()
                    .owner_observations()
                    .iter()
                    .find_map(|observation| observation.multi_root_boundary_v1())
                    .unwrap();
                assert_eq!(boundary.slots().len(), roots);
                let rule = source
                    .ownership()
                    .owner_observations()
                    .iter()
                    .find_map(|observation| observation.multi_root_normal_transport_v1())
                    .unwrap();
                let incoming: Vec<_> = boundary
                    .slots()
                    .iter()
                    .map(|slot| {
                        (
                            slot.root(),
                            UnitOwnerHeaderSourceV1::Backedge(slot.first_body_output()),
                        )
                    })
                    .collect();
                let partition = boundary
                    .condition_false_partition(boundary.statement(), &incoming)
                    .unwrap();
                assert_eq!(partition.live_roots().len(), live);
                assert_eq!(partition.retirements().len(), roots - live);
                assert_eq!(
                    boundary
                        .condition_false_suffix_retirements(boundary.statement(), &incoming)
                        .is_some(),
                    live > 0
                );

                let (program, entry) = source.lower_entry(source.entry()).unwrap();
                crate::ssa::verify::verify_program(&program).unwrap();
                let llvm =
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
                let replacements: Vec<_> = run
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
                    .collect();
                assert_eq!(replacements.len(), roots);
                let body = run.block(replacements[0].block).unwrap();
                let TerminatorKind::Branch(backedge) = &body.terminator.as_ref().unwrap().kind
                else {
                    panic!("normal backedge")
                };
                let header = run.block(backedge.target).unwrap();
                let TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } = &header.terminator.as_ref().unwrap().kind
                else {
                    panic!("header condition")
                };
                assert_eq!(when_true.target, body.id);
                let entries: Vec<_> = run
                    .blocks
                    .iter()
                    .filter_map(|block| match &block.terminator.as_ref()?.kind {
                        TerminatorKind::Branch(edge)
                            if edge.target == header.id && block.id != body.id =>
                        {
                            Some(edge)
                        }
                        _ => None,
                    })
                    .collect();
                let [initial_edge] = entries.as_slice() else {
                    panic!("one entry predecessor")
                };
                let exit = run.block(when_false.target).unwrap();
                assert!(matches!(
                    exit.terminator.as_ref().unwrap().kind,
                    TerminatorKind::Return { .. }
                ));
                let mut positions = Vec::new();
                for (replace, step) in replacements.iter().zip(rule.steps()) {
                    let call_span = source
                        .parsed
                        .ast()
                        .expressions()
                        .get(step.call().expression())
                        .unwrap()
                        .span();
                    assert_eq!(replace.origin.span(), call_span);
                    let Operation::RootReplace {
                        owner, replacement, ..
                    } = replace.operation
                    else {
                        unreachable!()
                    };
                    let UnitOwnerOrigin::String(producer) = boundary
                        .body_catalog()
                        .definition(step.producer())
                        .unwrap()
                        .origin()
                    else {
                        panic!("actual body String");
                    };
                    let producer_span = source
                        .parsed
                        .ast()
                        .expressions()
                        .get(producer.expression())
                        .unwrap()
                        .span();
                    let produced = run
                        .instructions
                        .iter()
                        .find(|instruction| {
                            instruction.block == body.id
                                && instruction.origin.span() == producer_span
                                && matches!(instruction.operation, Operation::StringLiteral { .. })
                        })
                        .unwrap();
                    assert_eq!(EntityId::Value(replacement), produced.results[0]);
                    let position = body
                        .parameters
                        .iter()
                        .position(|value| *value == EntityId::Value(owner))
                        .unwrap();
                    assert!(!positions.contains(&position));
                    positions.push(position);
                    assert_eq!(backedge.arguments[position], replace.results[0]);
                    assert_eq!(when_true.arguments[position], header.parameters[position]);
                    let slot = boundary
                        .slots()
                        .iter()
                        .find(|slot| slot.root() == step.header_root())
                        .unwrap();
                    let UnitOwnerOrigin::String(initial) = boundary
                        .input_catalog()
                        .definition(slot.entry())
                        .unwrap()
                        .origin()
                    else {
                        panic!("actual original String")
                    };
                    let initial_span = source
                        .parsed
                        .ast()
                        .expressions()
                        .get(initial.expression())
                        .unwrap()
                        .span();
                    let initial = run
                        .instructions
                        .iter()
                        .find(|instruction| {
                            instruction.origin.span() == initial_span
                                && matches!(instruction.operation, Operation::StringLiteral { .. })
                        })
                        .unwrap();
                    assert_eq!(initial_edge.arguments[position], initial.results[0]);
                    let body_drops: Vec<_> = run.instructions.iter().filter(|i| i.block == body.id && matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == replace.results[1])).collect();
                    assert_eq!(body_drops.len(), 1);
                    assert!(
                        body.instructions
                            .iter()
                            .position(|id| *id == replace.id)
                            .unwrap()
                            < body
                                .instructions
                                .iter()
                                .position(|id| *id == body_drops[0].id)
                                .unwrap()
                    );
                    let exit_position = when_false
                        .arguments
                        .iter()
                        .position(|value| *value == header.parameters[position])
                        .unwrap();
                    let exit_value = exit.parameters[exit_position];
                    assert_eq!(run.instructions.iter().filter(|i| i.block == exit.id && matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == exit_value)).count(), 1);
                }
                assert_eq!(positions.len(), roots);
                assert_eq!(
                    run.instructions
                        .iter()
                        .filter(|instruction| matches!(
                            instruction.operation,
                            Operation::Drop { .. }
                        ))
                        .count(),
                    roots * 2
                );
                // Assert actual LLVM two-predecessor owner phis, not only the word "phi".
                let loop_phis = llvm
                    .lines()
                    .filter(|line| {
                        line.contains(" = phi %koven.string.")
                            && line.matches(" ], [ ").count() == 1
                    })
                    .count();
                assert_eq!(loop_phis, roots);
            }
        }
    }
}

#[test]
fn multi_root_header_native_zero_iteration_consumes_live_suffix() {
    // Pure immutable Name guard cannot terminate after a true iteration in this slice.
    // Do not pretend this zero-iteration native run proves an executed backedge.
    let source = UnitSource::analyze("multi-root-header-native.ko", &source(3, true, 3)).unwrap();
    source.assert_frontend_clean();
    super::assert_native_output(&source, b"");
}
