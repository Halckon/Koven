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
                for constant in [false, true] {
                    let mut text = source(roots, reverse, live);
                    if constant {
                        text = format!(
                            "const val OFF: Boolean = false\n{}",
                            text.replace("run(false)", "run(OFF)")
                        );
                    }
                    let source = UnitSource::analyze("multi-root-header.ko", &text).unwrap();
                    assert_eq!(
                        matches!(source.products, super::OwnedProducts::Constant(..)),
                        constant
                    );
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

                    let (lowered, actions) =
                        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
                    assert_eq!(
                        actions
                            .iter()
                            .filter(|(kind, _)| *kind == "multi-root-exclusive-plan")
                            .copied()
                            .collect::<Vec<_>>(),
                        [("multi-root-exclusive-plan", roots)]
                    );
                    let (program, entry) = lowered.unwrap();
                    assert_eq!(
                        actions
                            .iter()
                            .filter(|(kind, _)| *kind == "multi-root-header-audit")
                            .copied()
                            .collect::<Vec<_>>(),
                        [("multi-root-header-audit", roots)]
                    );
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
                                    && matches!(
                                        instruction.operation,
                                        Operation::StringLiteral { .. }
                                    )
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
                                    && matches!(
                                        instruction.operation,
                                        Operation::StringLiteral { .. }
                                    )
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
}

#[test]
fn multi_root_header_native_zero_iteration_consumes_live_suffix() {
    // Pure immutable Name guard cannot terminate after a true iteration in this slice.
    // Do not pretend this zero-iteration native run proves an executed backedge.
    let source = UnitSource::analyze("multi-root-header-native.ko", &source(3, true, 3)).unwrap();
    source.assert_frontend_clean();
    super::assert_native_output(&source, b"");
}

#[test]
fn multi_root_finite_native_backedges_do_not_expand_new_product_scope() {
    use super::OwnedProducts;
    for roots in [2, 3] {
        for rounds in [0, 1, 3] {
            for constant in [false, true] {
                let declarations = (0..roots)
                    .map(|i| format!("var r{i} = \"entry{i}\";"))
                    .collect::<String>();
                let body = (0..roots)
                    .rev()
                    .map(|i| format!("val next{i} = r{i} + \"x\"; val old{i} = replace(&r{i}, next{i}); println(old{i});"))
                    .collect::<String>();
                let suffix = (0..roots)
                    .map(|i| format!("println(r{i});"))
                    .collect::<String>();
                let mut text = format!(
                    "fun run(rounds: Int) {{ {declarations} var remaining = rounds; while(remaining > 0) {{ {body} remaining = remaining - 1 }}; {suffix} }}\nfun entry() {{ run({rounds}) }}"
                );
                if constant {
                    text = format!(
                        "const val ROUNDS: Int = {rounds}\n{}",
                        text.replace(&format!("run({rounds})"), "run(ROUNDS)")
                    );
                }
                let source = UnitSource::analyze("multi-root-finite-native.ko", &text).unwrap();
                source.assert_frontend_clean();
                assert_eq!(
                    matches!(source.products, OwnedProducts::Constant(..)),
                    constant
                );
                // Counted guard, counter writes and printed old results are outside the
                // new restricted product. Success here must not silently authorize it.
                assert!(
                    source
                        .ownership()
                        .owner_observations()
                        .iter()
                        .all(|o| o.multi_root_boundary_v1().is_none())
                );
                let (lowered, actions) =
                    crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
                let (program, entry) = lowered.unwrap();
                assert!(
                    actions
                        .iter()
                        .all(|(kind, _)| *kind != "multi-root-header-audit")
                );
                crate::ssa::verify::verify_program(&program).unwrap();
                let llvm =
                    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                assert!(
                    llvm.lines()
                        .filter(|line| line.contains(" = phi %koven.string.")
                            && line.matches(" ], [ ").count() == 1)
                        .count()
                        >= roots
                );
                let mut expected = String::new();
                for round in 0..rounds {
                    for root in (0..roots).rev() {
                        expected.push_str(&format!("entry{root}{}\n", "x".repeat(round as usize)));
                    }
                }
                for root in 0..roots {
                    expected.push_str(&format!("entry{root}{}\n", "x".repeat(rounds as usize)));
                }
                super::assert_native_output(&source, expected.as_bytes());
            }
        }
    }
}

#[test]
fn ci_only_multi_root_actual_receipt_cannot_be_skipped_or_rebound() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let source = UnitSource::analyze("multi-root-receipt-fault.ko", &source(2, false, 2)).unwrap();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::ConflictingMultiRootAuthority,
        OwnerFault::MissingMultiRootGeometry,
        OwnerFault::MissingMultiRootGuard,
        OwnerFault::WrongMultiRootRoot,
        OwnerFault::MissingMultiRootDropReceipt,
    ] {
        assert!(with_owner_fault(fault, || source.lower_entry(source.entry())).is_err());
    }
}

#[test]
fn multi_root_receipt_consumes_actual_aliased_entry() {
    for depth in [1, 2, 4] {
        for live in [0, 1, 2] {
            let mut prefix = String::from("val seed0 = \"entry0\";");
            for index in 1..depth {
                prefix.push_str(&format!("val seed{index} = seed{};", index - 1));
            }
            prefix.push_str(&format!("var r0 = seed{};", depth - 1));
            let text = source(2, true, live).replace("var r0 = \"entry0\";", &prefix);
            let source = UnitSource::analyze("multi-root-alias-entry.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert!(
                source
                    .ownership()
                    .owner_observations()
                    .iter()
                    .any(|o| o.multi_root_boundary_v1().is_some())
            );
            let (lowered, actions) =
                crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
            let (program, entry) = lowered.unwrap();
            crate::ssa::verify::verify_program(&program).unwrap();
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "multi-root-header-audit")
                    .count(),
                1
            );
            super::assert_native_output(&source, b"");
        }
    }
}

#[test]
fn multi_root_receipt_preserves_completed_prefix_before_root_reinitialization() {
    for live in [0, 2] {
        let text = source(2, false, live).replace(
            "}\nfun entry",
            "r0 = \"later\"; val again = r0; }\nfun entry",
        );
        let source = UnitSource::analyze("multi-root-later-root.ko", &text).unwrap();
        source.assert_frontend_clean();
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "multi-root-header-audit")
                .count(),
            1
        );
    }
}

#[test]
fn multi_root_consumes_actual_scalar_prefix_diamonds() {
    let prefix = "if(choose) { val n = 1 } else { val n = 2 };";
    for choose in [false, true] {
        let text = source(2, true, 1)
            .replace("flag: Boolean", "flag: Boolean, choose: Boolean")
            .replace("run(false)", &format!("run(false, {choose})"))
            .replace("while(flag)", &format!("{prefix} while(flag)"));
        let source = UnitSource::analyze("multi-root-prefix-cfg.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert!(
            source
                .ownership()
                .owner_observations()
                .iter()
                .any(|o| o.multi_root_boundary_v1().is_some())
        );
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        assert!(
            actions
                .iter()
                .any(|(kind, _)| *kind == "multi-root-header-audit")
        );
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "multi-root-cross-block-entry")
                .count(),
            2
        );
        super::assert_native_output(&source, b"");
    }
}

#[test]
fn multi_root_receipt_binds_actual_local_guard_slot() {
    let text = source(2, false, 2).replace("while(flag)", "var again = flag; while(again)");
    let source = UnitSource::analyze("multi-root-local-guard.ko", &text).unwrap();
    source.assert_frontend_clean();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "multi-root-header-audit")
            .count(),
        1
    );
}

#[test]
fn multi_root_receipt_substitutes_sparse_entry_catalog_by_root() {
    for live in [0, 1, 3] {
        let text =
            source(3, true, live).replace("var r0", "val discarded = \"not-a-root\"; var r0");
        let source = UnitSource::analyze("multi-root-sparse-entry.ko", &text).unwrap();
        source.assert_frontend_clean();
        let boundary = source
            .ownership()
            .owner_observations()
            .iter()
            .find_map(|o| o.multi_root_boundary_v1())
            .unwrap();
        assert!(boundary.input_catalog().definitions().len() > boundary.slots().len());
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|(kind, _)| *kind == "multi-root-header-audit")
                .count(),
            1
        );
    }
}

#[test]
fn multi_root_aliased_entry_with_prior_read_stays_on_legacy_path() {
    let text = source(2, false, 2).replace(
        "var r0 = \"entry0\";",
        "val seed = \"entry0\"; println(seed); var r0 = seed;",
    );
    let source = UnitSource::analyze("multi-root-read-entry.ko", &text).unwrap();
    source.assert_frontend_clean();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert!(
        actions
            .iter()
            .all(|(kind, _)| *kind != "multi-root-header-audit")
    );
    super::assert_native_output(&source, b"entry0\n");
}

#[test]
fn multi_root_prefix_root_read_does_not_gain_cross_block_receipt() {
    let text = source(2, false, 2).replace("while(flag)", "if(flag) { println(r0) }; while(flag)");
    let source = UnitSource::analyze("multi-root-prefix-read.ko", &text).unwrap();
    source.assert_frontend_clean();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // The frontend already withholds this product for the root-touching branch.
    assert!(
        source
            .ownership()
            .owner_observations()
            .iter()
            .all(|o| o.multi_root_boundary_v1().is_none())
    );
    assert!(
        actions
            .iter()
            .all(|(kind, _)| *kind != "multi-root-header-audit")
    );
    super::assert_native_output(&source, b"");
}

#[test]
fn multi_root_consumes_alias_move_across_scalar_diamond() {
    for depth in [1, 12] {
        for choose in [false, true] {
            let text = source(2, true, 1)
            .replace("flag: Boolean", "flag: Boolean, choose: Boolean")
            .replace("run(false)", &format!("run(false, {choose})"))
            .replace("var r0 = \"entry0\";", "val seed = \"entry0\"; if(choose) { val n = 1 } else { val n = 2 }; val moved = seed; var r0 = moved;");
            let diamond = "if(choose) { val n = 1 } else { val n = 2 };";
            let text = text.replace(diamond, &diamond.repeat(depth));
            let source = UnitSource::analyze("multi-root-cross-block-move.ko", &text).unwrap();
            source.assert_frontend_clean();
            assert!(
                source
                    .ownership()
                    .owner_observations()
                    .iter()
                    .any(|o| o.multi_root_boundary_v1().is_some())
            );
            let (lowered, actions) =
                crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
            let (program, entry) = lowered.unwrap();
            crate::ssa::verify::verify_program(&program).unwrap();
            crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            assert!(
                actions
                    .iter()
                    .any(|(kind, _)| *kind == "multi-root-header-audit")
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "multi-root-cross-block-entry")
                    .count(),
                1
            );
            super::assert_native_output(&source, b"");
        }
    }
}

#[test]
fn multi_root_exit_suffix_consumes_across_copyable_literal_declarations() {
    for literal in ["1", "true"] {
        let text = source(2, true, 2)
            .replace("val end1", &format!("val firstGap = {literal}; val end1"))
            .replace("val end0", &format!("val secondGap = {literal}; val end0"));
        let source = UnitSource::analyze("multi-root-suffix-literal.ko", &text).unwrap();
        source.assert_frontend_clean();
        let (lowered, actions) =
            crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
        let (program, entry) =
            lowered.unwrap_or_else(|error| panic!("literal {literal}: {error:?}"));
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        assert!(
            actions
                .iter()
                .any(|(kind, _)| *kind == "multi-root-header-audit")
        );
        super::assert_native_output(&source, b"");
    }
}

#[test]
fn multi_root_new_product_executes_terminal_false_backedge() {
    for roots in [2, 3] {
        for live in [0, 1, roots] {
            for initial in [false, true] {
                let text = source(roots, true, live)
                    .replace("while(flag)", "var again = flag; while(again)")
                    .replace(" };", " again = false; };")
                    .replace("run(false)", &format!("run({initial})"));
                let source = UnitSource::analyze("multi-root-terminal-false.ko", &text).unwrap();
                source.assert_frontend_clean();
                let boundary = source
                    .ownership()
                    .owner_observations()
                    .iter()
                    .find_map(|o| o.multi_root_boundary_v1())
                    .expect("real new product");
                assert!(boundary.guard_reset().is_some(), "{text}");
                let (lowered, actions) =
                    crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
                let (program, entry) = lowered.unwrap();
                crate::ssa::verify::verify_program(&program).unwrap();
                crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                assert!(
                    actions
                        .iter()
                        .any(|(kind, _)| *kind == "multi-root-header-audit")
                );
                super::assert_native_output(&source, b"");
            }
        }
    }
}

#[test]
fn multi_root_counter_tail_preserves_existing_lowering_without_new_authority() {
    // Isolate the scalar tail: root producers and old-value retirement are exactly
    // the already-supported shape, unlike the concatenating/printing loop fixture.
    for roots in [2, 3] {
        for rounds in [i32::MIN, -1, 0, 1, 2, 5] {
            for named_guard in [false, true] {
                let (header, tail) = if named_guard {
                    (
                        "var again = remaining > 0; while(again)",
                        "remaining = remaining - 1; again = remaining > 0;",
                    )
                } else {
                    ("while(remaining > 0)", "remaining = remaining - 1;")
                };
                let text = source(roots, true, 0)
                    .replace("flag: Boolean", "rounds: Int")
                    .replace("while(flag)", &format!("var remaining = rounds; {header}"))
                    .replace(
                        " };",
                        &format!(" {tail} }}; if(remaining == 0) {{ println(\"zero\") }};"),
                    )
                    .replace("run(false)", &format!("run({rounds})"));
                let source = UnitSource::analyze("multi-root-counter-tail.ko", &text).unwrap();
                source.assert_frontend_clean();
                assert!(
                    source
                        .ownership()
                        .owner_observations()
                        .iter()
                        .all(|o| o.multi_root_boundary_v1().is_none()),
                    "{text}"
                );
                let (lowered, actions) =
                    crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
                let (program, entry) = lowered.unwrap();
                assert_eq!(
                    actions
                        .iter()
                        .filter(|(kind, _)| *kind == "counter-tail-owner-audit")
                        .count(),
                    usize::from(named_guard),
                    "{text}"
                );
                assert_eq!(
                    source
                        .ownership()
                        .owner_observations()
                        .iter()
                        .any(|o| o.counter_tail_evidence_v1().is_some()),
                    named_guard
                );
                assert_eq!(
                    actions
                        .iter()
                        .filter(|(kind, _)| *kind == "checked-expression-edge-receipt")
                        .count(),
                    1,
                    "{text}"
                );
                crate::ssa::verify::verify_program(&program).unwrap();
                let llvm =
                    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                assert!(
                    llvm.lines()
                        .filter(|line| line.contains(" = phi %koven.string.")
                            && line.matches(" ], [ ").count() == 1)
                        .count()
                        >= roots
                );
                assert!(
                    actions
                        .iter()
                        .all(|(kind, _)| *kind != "multi-root-header-audit")
                );
                // Positive initial counts must traverse backedges to reach zero.
                // This is finite native evidence, not allocation counting or induction.
                super::assert_native_output(&source, if rounds < 0 { b"" } else { b"zero\n" });
            }
        }
    }
}

#[test]
fn ci_only_checked_expression_receipt_requires_actual_edges_and_operands() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let text = source(2, true, 0)
        .replace("while(flag)", "var remaining = 2; while(remaining > 0)")
        .replace(" };", " remaining = remaining - 1; };");
    let source = UnitSource::analyze("checked-edge-fault.ko", &text).unwrap();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::MissingCheckedExpressionEdge,
        OwnerFault::WrongCheckedExpressionEdgeOperand,
        OwnerFault::ReversedCheckedExpressionEdges,
    ] {
        assert!(with_owner_fault(fault, || source.lower_entry(source.entry())).is_err());
    }
}

#[test]
fn checked_expression_receipts_do_not_need_header_authority() {
    let text = source(2, true, 0)
        .replace("flag: Boolean", "rounds: Int")
        .replace(
            "while(flag)",
            "var value = rounds + 1; while(value > rounds)",
        )
        .replace(
            " };",
            " value = value - 1; }; val output = value * 2; if(output == 6) { println(\"ok\") };",
        )
        .replace("run(false)", "run(3)");
    let source = UnitSource::analyze("checked-edge-independent.ko", &text).unwrap();
    source.assert_frontend_clean();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "checked-expression-edge-receipt")
            .count(),
        3
    );
    assert!(
        actions
            .iter()
            .all(|(kind, _)| *kind != "multi-root-header-audit")
    );
    super::assert_native_output(&source, b"ok\n");
}

#[test]
fn counter_tail_audit_allows_scalar_reuse_after_loop() {
    let text = source(2, true, 0)
        .replace(
            "while(flag)",
            "var remaining = 2; var again = remaining > 0; while(again)",
        )
        .replace(
            " };",
            " remaining = remaining - 1; again = remaining > 0; }; remaining = 7; again = false;",
        );
    let source = UnitSource::analyze("counter-reuse.ko", &text).unwrap();
    source.assert_frontend_clean();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "counter-tail-owner-audit")
            .count(),
        1
    );
    super::assert_native_output(&source, b"");
}

#[test]
fn ci_only_counter_tail_requires_writeback_comparison_and_success_backedge() {
    use crate::ssa::{OwnerFault, with_owner_fault};
    let text = source(2, true, 0)
        .replace(
            "while(flag)",
            "var remaining = 2; var again = remaining > 0; while(again)",
        )
        .replace(
            " };",
            " remaining = remaining - 1; again = remaining > 0; };",
        );
    let source = UnitSource::analyze("counter-tail-fault.ko", &text).unwrap();
    source.assert_frontend_clean();
    for fault in [
        OwnerFault::CounterTailLateInitialization,
        OwnerFault::CounterTailWrongBackedge,
        OwnerFault::CounterTailWrongCounterWrite,
        OwnerFault::CounterTailWrongGuardWrite,
        OwnerFault::CounterTailStaleComparison,
    ] {
        assert!(with_owner_fault(fault, || source.lower_entry(source.entry())).is_err());
    }
}

#[test]
fn counter_tail_inout_parameter_keeps_existing_lowering_without_new_audit() {
    let text = source(2, true, 0)
        .replace("flag: Boolean", "inout remaining: Int")
        .replace("while(flag)", "var again = remaining > 0; while(again)")
        .replace(
            " };",
            " remaining = remaining - 1; again = remaining > 0; };",
        )
        .replace("run(false)", "var n = 2; run(&n)");
    let source = UnitSource::analyze("counter-inout-fallback.ko", &text).unwrap();
    source.assert_frontend_clean();
    assert!(
        source
            .ownership()
            .owner_observations()
            .iter()
            .all(|o| o.counter_tail_evidence_v1().is_none())
    );
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert!(
        actions
            .iter()
            .all(|(kind, _)| *kind != "counter-tail-owner-audit")
    );
    super::assert_native_output(&source, b"");
}
