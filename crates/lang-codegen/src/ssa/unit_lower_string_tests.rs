use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{EntityId, Operation, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn dropped_grouped_literal_is_not_carried_into_later_control_flow() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/entry.ko",
        "package test\nfun entry(own flag: Boolean): Unit { val printed = println(((\"x\")))\nif (flag) {} }",
    );
    let inputs = [SourceUnitInput::new("root", "test/entry.ko", source, &file)];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let (program, entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("all transparent aliases end with the temporary owner");
    let function = program.modules[0].function(entry).unwrap();
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1
    );
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn lowers_standard_print_and_abort_calls_in_unit_source() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/entry.ko",
        "package test\n\
         fun entry(): Unit {\n\
             val printed = println(\"before abort\")\n\
             val stopped = error(\"failure\")\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "test/entry.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("standard print and abort calls lower to verified unit SSA");

    let function = &program.modules[0].functions[0];
    let printed_loan = function
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::PrintString { value } => Some(value),
            _ => None,
        })
        .expect("println emits a string print operation");
    assert!(function.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::BorrowEnd { loan } if loan == printed_loan
    )));
    assert!(function.blocks.iter().any(|block| {
        block
            .terminator
            .as_ref()
            .is_some_and(|terminator| matches!(terminator.kind, TerminatorKind::Abort))
    }));
}

#[test]
fn preserves_a_nested_abort_used_as_a_borrow_argument() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/entry.ko",
        "package test\nfun entry(): Unit { val stopped = println(error(\"stop\")) }",
    );
    let inputs = [SourceUnitInput::new("root", "test/entry.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("a diverging Borrow argument propagates its inner Abort");

    let function = &program.modules[0].functions[0];
    assert!(function.blocks.iter().any(|block| {
        block
            .terminator
            .as_ref()
            .is_some_and(|terminator| matches!(terminator.kind, TerminatorKind::Abort))
    }));
    assert!(
        !function
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::PrintString { .. })),
        "the outer println is unreachable after the nested Abort"
    );
}

#[test]
fn returns_literal_concat_after_dropping_only_its_operand_owners() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/entry.ko",
        "package test\nfun entry(): String = \"left\" + \"right\"",
    );
    let inputs = [SourceUnitInput::new("root", "test/entry.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("literal concat lowers to verified SSA");

    let function = &program.modules[0].functions[0];
    let literals = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
        .map(|instruction| instruction.results[0])
        .collect::<Vec<_>>();
    assert_eq!(literals.len(), 2);
    let (concat_index, concat_result) = function
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match instruction.operation {
            Operation::StringConcat { left, right }
                if [left, right] == [literals[0], literals[1]] =>
            {
                Some((index, instruction.results[0]))
            }
            _ => None,
        })
        .expect("concat reads both literal owners in source order");
    assert_eq!(
        drop_owners(&function.instructions[concat_index + 1..concat_index + 3]),
        vec![literals[1], literals[0]],
        "temporary String operands drop in reverse evaluation order"
    );
    let returned = function
        .blocks
        .iter()
        .find_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Return { values }) => Some(values.as_slice()),
                _ => None,
            },
        )
        .expect("entry returns the concat owner");
    let EntityId::Value(concat_result) = concat_result else {
        panic!("concat result is a value");
    };
    assert_eq!(returned, &[concat_result]);
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        2,
        "the returned concat owner is not dropped"
    );
}

#[test]
fn lowers_cross_file_concat_equality_and_value_delivery_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun suffix(): String = \"!\"\n\
         fun identity(own input: String): String = input",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Boolean {\n\
             val left = \"K\"\n\
             val joined = left + p.suffix()\n\
             val same = joined == \"K!\"\n\
             val different = joined != \"other\"\n\
             val consumed = p.identity(joined)\n\
             return same == different\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry_declaration = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry_declaration,
    )
    .expect("cross-file String binary expressions lower");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry_declaration,
    )
    .expect("input permutation preserves String lowering");
    assert_text_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("q.entry"))
        .expect("entry function exists");
    let suffix = module
        .functions
        .iter()
        .find(|function| function.name.contains("p.suffix"))
        .expect("suffix function exists")
        .id;
    let identity = module
        .functions
        .iter()
        .find(|function| function.name.contains("p.identity"))
        .expect("identity function exists")
        .id;
    let suffix_result = entry
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } if callee == suffix => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("suffix call result exists");
    let left = string_literal_result(entry, b"K");
    let expected = string_literal_result(entry, b"K!");
    let other = string_literal_result(entry, b"other");
    let (concat_index, joined) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match instruction.operation {
            Operation::StringConcat {
                left: actual_left,
                right: actual_right,
            } if [actual_left, actual_right] == [left, suffix_result] => {
                Some((index, instruction.results[0]))
            }
            _ => None,
        })
        .expect("concat reads local and call result in source order");
    assert_eq!(
        drop_owners(&entry.instructions[concat_index + 1..concat_index + 3]),
        vec![suffix_result, left]
    );

    let equalities = entry
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| matches!(instruction.operation, Operation::StringEqual { .. }))
        .collect::<Vec<_>>();
    assert_eq!(equalities.len(), 2);
    assert!(matches!(
        equalities[0].1.operation,
        Operation::StringEqual { left, right } if [left, right] == [joined, expected]
    ));
    assert_eq!(
        drop_owners(&entry.instructions[equalities[0].0 + 1..equalities[0].0 + 2]),
        vec![expected]
    );
    assert!(matches!(
        equalities[1].1.operation,
        Operation::StringEqual { left, right } if [left, right] == [joined, other]
    ));
    assert_eq!(
        drop_owners(&entry.instructions[equalities[1].0 + 1..equalities[1].0 + 2]),
        vec![other]
    );
    let (not_index, not) = entry
        .instructions
        .iter()
        .enumerate()
        .find(|(_, instruction)| matches!(instruction.operation, Operation::BooleanNot { .. }))
        .expect("not-equal negates StringEqual");
    assert_eq!(not_index, equalities[1].0 + 2);
    let EntityId::Value(second_equal) = equalities[1].1.results[0] else {
        panic!("StringEqual result is Boolean");
    };
    assert!(matches!(
        not.operation,
        Operation::BooleanNot { operand } if operand == second_equal
    ));
    let identity_arguments = entry
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee, arguments, ..
            } if *callee == identity => Some(arguments.as_slice()),
            _ => None,
        })
        .expect("identity call exists");
    assert_eq!(identity_arguments, &[joined]);
    assert!(
        !entry.instructions[..concat_index]
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
}

fn string_literal_result(function: &super::model::Function, bytes: &[u8]) -> EntityId {
    function
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::StringLiteral { bytes: actual, .. } if actual == bytes => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("expected String literal exists")
}

fn drop_owners(instructions: &[super::model::Instruction]) -> Vec<EntityId> {
    instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Drop { owner } => Some(EntityId::Value(owner)),
            _ => None,
        })
        .collect()
}

pub(in crate::ssa) mod lower_frontend_nullable_unit_comparison_tests {
//! Stage B: eleven unchanged source instances, eighteen independent Unit roots.
//! The five paired legacy positives are retired after Stage A dual-platform evidence:
//! https://github.com/Halckon/Koven/actions/runs/40667339899 (PR #208).
//! This removes their eleven-source whole-file positive coverage; these selected-root
//! SSA/LLVM oracles do not establish whole-file, exact CFG, or native equivalence.
//! Unreachable-inline registration and inline/borrowed nullable rejections remain legacy.
//! Admission failures are hard failures.
use super::super::{
    model::{
        EntityId, EntityType, LoanKind, Operation, Origin, Program, SsaTypeKind, TerminatorKind,
    },
    render::render_program,
    verify::verify_program,
};
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};
use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

fn analyze(text: &str) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict unchanged Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one unchanged MemoryDocument");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    unit
}

// Every original function is independently admitted as an entry, including empty helpers.
// Return inspect's program only after all roots have passed both complete verifiers.
fn lower_roots(unit: &UnitSource, roots: &[&str], call_count: usize) -> Program {
    let mut inspect = None;
    for &name in roots {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let span = unit.declaration_span(declaration);
        let (program, root) = unit
            .lower_entry(declaration)
            .unwrap_or_else(|error| panic!("original {name} Unit root failed: {error:?}"));
        let module = program.module(root.module()).expect("root module");
        let function = module.function(root).expect("root function");
        assert_eq!(function.origin, Origin::Source(span));
        assert!(!function.blocks.is_empty());
        if name == "inspect" {
            let views = function
                .blocks
                .iter()
                .filter_map(|block| match &block.terminator.as_ref().unwrap().kind {
                    TerminatorKind::NullableBranch { view, .. } => Some(*view),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(!views.is_empty(), "original nullable proof exists");
            for instruction in &function.instructions {
                match instruction.operation {
                    Operation::SharedRetain { owner } | Operation::SharedPayloadPlace { owner } => {
                        let EntityId::Loan(loan) = owner else {
                            panic!("Rc uses the original non-null view")
                        };
                        assert!(views.contains(&loan));
                    }
                    Operation::NullableTake { proof, .. } => assert!(views.contains(&proof)),
                    _ => {}
                }
            }
            let calls = function
                .instructions
                .iter()
                .filter_map(|instruction| {
                    if let Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    } = &instruction.operation
                    {
                        Some((instruction, *callee, receiver, arguments))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(calls.len(), call_count);
            if let Some(&callee_name) = roots.iter().find(|&&root| root != "inspect") {
                let callee_span =
                    unit.declaration_span(unit.declaration(callee_name, SymbolKind::Function));
                for (call, target, receiver, arguments) in calls {
                    let callee = module.function(target).expect("actual call target");
                    assert_eq!(callee.origin, Origin::Source(callee_span));
                    assert!(receiver.is_none());
                    assert_eq!(
                        unit.sources().slice(call.origin.span()),
                        Ok(format!("{callee_name}(node)").as_str())
                    );
                    let [argument] = arguments.as_slice() else {
                        panic!("one original argument")
                    };
                    let [parameter] = callee.blocks[0].parameters.as_slice() else {
                        panic!("one original parameter")
                    };
                    assert_eq!(
                        function.entity(*argument).unwrap().ty,
                        callee.entity(*parameter).unwrap().ty
                    );
                    if callee_name == "observe" {
                        let EntityId::Loan(loan) = argument else {
                            panic!("observe borrows a proven view")
                        };
                        assert!(views.contains(loan));
                    } else {
                        assert_eq!(callee_name, "consume");
                        assert!(function.instructions.iter().any(|instruction| matches!(
                            instruction.operation,
                            Operation::NullableTake { .. }
                        )
                            && instruction.results == [*argument]));
                    }
                }
            }
            for block in &function.blocks {
                if let TerminatorKind::NullableBranch {
                    owner,
                    view,
                    when_non_null,
                    ..
                } = &block.terminator.as_ref().unwrap().kind
                {
                    let owner_type = function
                        .entity(EntityId::Value(*owner))
                        .unwrap()
                        .ty
                        .semantic_type();
                    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(owner_type)
                    else {
                        panic!("original nullable owner")
                    };
                    assert_eq!(
                        function.entity(EntityId::Loan(*view)).unwrap().ty,
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: *inner
                        }
                    );
                    assert_eq!(
                        function
                            .block(when_non_null.target)
                            .unwrap()
                            .parameters
                            .last(),
                        Some(&EntityId::Loan(*view))
                    );
                }
            }
        }
        verify_program(&program).expect("complete original-root SSA verifier");
        render_verified_program(&program).expect("complete original-root LLVM verifier");
        if name == "inspect" {
            inspect = Some(program);
        }
    }
    inspect.expect("original inspect root selected")
}

#[test]
fn lowers_nullable_class_parameter_smart_cast_to_non_null_borrow_view_unit_comparison() {
    let analysis = analyze(
        "class Node()\n\
         fun observe(node: Node): Unit {}\n\
         fun inspect(own node: Node?): Unit {\n\
             if (node != null) { observe(node) }\n\
         }",
    );
    assert!(
        analysis.types().diagnostics().is_empty(),
        "{:?}",
        analysis.types().diagnostics()
    );
    assert!(
        analysis.ownership().diagnostics().is_empty(),
        "{:?}",
        analysis.ownership().diagnostics()
    );
    let program = lower_roots(&analysis, &["observe", "inspect"], 1);
    let ssa = render_program(&program);
    assert_diagnostic!(ssa.contains("nullable_handle<"), "{ssa}");
    assert_diagnostic!(ssa.contains("nullable.branch"), "{ssa}");
    assert_diagnostic!(ssa.contains("view %l"), "{ssa}");
    let llvm = render_verified_program(&program).expect("nullable smart cast LLVM");
    assert_diagnostic!(llvm.contains("icmp eq ptr"), "{llvm}");
    assert_diagnostic!(llvm.contains("phi ptr"), "{llvm}");
}

#[test]
fn lowers_nullable_rc_operations_through_the_proven_non_null_view_unit_comparison() {
    let analysis = analyze(
        "fun inspect(own owner: Rc<Int>?): Unit {\n\
             if (owner != null) {\n\
                 val retained = owner.share()\n\
                 val copied = owner.value\n\
             }\n\
         }",
    );
    assert!(
        analysis.types().diagnostics().is_empty(),
        "{:?}",
        analysis.types().diagnostics()
    );
    assert!(
        analysis.ownership().diagnostics().is_empty(),
        "{:?}",
        analysis.ownership().diagnostics()
    );
    let program = lower_roots(&analysis, &["inspect"], 0);
    let span = analysis.declaration_span(analysis.declaration("inspect", SymbolKind::Function));
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(span))
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        panic!("one original Rc inspect function")
    };
    let views = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::NullableBranch { view, .. } => Some(*view),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [view] = views.as_slice() else {
        panic!("one original Rc non-null proof")
    };
    // Presence and identity are structural: rendered text alone cannot witness either use.
    for expected in [
        Operation::SharedRetain {
            owner: EntityId::Loan(*view),
        },
        Operation::SharedPayloadPlace {
            owner: EntityId::Loan(*view),
        },
    ] {
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|instruction| instruction.operation == expected)
                .count(),
            1,
            "the original Rc source must perform {expected:?} once through its proof"
        );
    }
    let ssa = render_program(&program);
    assert_diagnostic!(ssa.contains("shared.retain %l"), "{ssa}");
    assert_diagnostic!(ssa.contains("shared.payload_place %l"), "{ssa}");
    assert_diagnostic!(ssa.contains("end_borrow"), "{ssa}");
    let llvm = render_verified_program(&program).expect("nullable Rc view LLVM");
    assert_diagnostic!(llvm.contains("icmp eq ptr"), "{llvm}");
    assert_diagnostic!(llvm.contains(".next = add i64"), "{llvm}");
}

#[test]
fn nullable_when_read_views_preserve_owned_subject_for_reuse_unit_comparison() {
    for inner in ["Node", "Box<Token>", "Rc<Int>"] {
        let analysis = analyze(&format!(
            "class Node()\nvalue class Token(val item: Int)\n\
             fun observe(node: {inner}): Unit {{}}\n\
             fun inspect(own node: {inner}?): Unit {{\n\
                 when (node) {{ null -> {{}}; else -> {{ observe(node) }} }}\n\
                 when (node) {{ null -> {{}}; else -> {{ observe(node) }} }}\n\
             }}"
        ));
        assert!(
            analysis.types().diagnostics().is_empty(),
            "{:?}",
            analysis.types().diagnostics()
        );
        assert!(
            analysis.ownership().diagnostics().is_empty(),
            "{:?}",
            analysis.ownership().diagnostics()
        );
        let program = lower_roots(&analysis, &["observe", "inspect"], 2);
        let ssa = render_program(&program);
        assert_eq_diagnostic!(ssa.matches("nullable.branch").count(), 2, "{ssa}");
        assert_diagnostic!(
            !ssa.contains("nullable.take"),
            "a view must not consume the owner: {ssa}"
        );
        assert_diagnostic!(
            !ssa.contains("shared.retain"),
            "read views must not share: {ssa}"
        );
        assert_diagnostic!(
            !ssa.contains("heap.allocate"),
            "proofs must not allocate: {ssa}"
        );
        let llvm = render_verified_program(&program)
            .expect("both nullable branches must pass LLVM lowering");
        assert_diagnostic!(!llvm.contains("call ptr @malloc"), "{llvm}");
        assert_diagnostic!(
            !llvm.contains("%koven.enum"),
            "pointer nullable needs no tag: {llvm}"
        );
    }
}

#[test]
fn nullable_when_consumes_inner_without_duplicate_owner_unit_comparison() {
    for inner in ["Node", "Box<Token>", "Rc<Int>"] {
        let analysis = analyze(&format!(
            "class Node()\nvalue class Token(val item: Int)\nfun consume(own node: {inner}): Unit {{}}\nfun inspect(own node: {inner}?): Unit {{ when (node) {{ null -> {{}}; else -> {{ consume(node) }} }} }}"
        ));
        assert!(
            analysis.types().diagnostics().is_empty(),
            "{:?}",
            analysis.types().diagnostics()
        );
        assert!(
            analysis.ownership().diagnostics().is_empty(),
            "{:?}",
            analysis.ownership().diagnostics()
        );
        let program = lower_roots(&analysis, &["consume", "inspect"], 1);
        let ssa = render_program(&program);
        assert_eq_diagnostic!(ssa.matches("nullable.take").count(), 1, "{ssa}");
        render_verified_program(&program).expect("unique extracted owner verifies in LLVM");
    }
}

#[test]
fn non_null_assertion_owned_pointer_uses_proven_take_and_direct_abort_unit_comparison() {
    // The assertion transfers the original nullable owner, without allocating or retaining it.
    for inner in ["Node", "Box<Token>", "Rc<Int>"] {
        let analysis = analyze(&format!(
            "class Node()\nvalue class Token(val item: Int)\nfun inspect(own node: {inner}?): {inner} = node!!"
        ));
        assert!(analysis.parsed().diagnostics().is_empty());
        assert!(analysis.names().names().diagnostics().is_empty());
        assert!(
            analysis.types().diagnostics().is_empty(),
            "{:?}",
            analysis.types().diagnostics()
        );
        assert!(
            analysis.ownership().diagnostics().is_empty(),
            "{:?}",
            analysis.ownership().diagnostics()
        );
        assert_eq!(analysis.ownership().non_null_assertions().len(), 1);
        let descriptor = analysis.ownership().non_null_assertions()[0].descriptor();
        let document = &analysis.names().names().index().source_units()[0];
        assert_eq!(descriptor.expression().source_unit(), document.id());
        assert_eq!(descriptor.operand().source_unit(), document.id());
        for (expression, expected) in [
            (descriptor.expression(), "node!!"),
            (descriptor.operand(), "node"),
        ] {
            let span = analysis
                .parsed()
                .ast()
                .expressions()
                .get(expression.expression())
                .unwrap()
                .span();
            assert_eq!(analysis.sources().slice(span), Ok(expected));
        }
        assert_eq!(
            analysis.sources().slice(descriptor.operator_span()),
            Ok("!!")
        );
        let program = lower_roots(&analysis, &["inspect"], 0);
        let ssa = render_program(&program);
        assert_eq_diagnostic!(ssa.matches("nullable.take").count(), 1, "{ssa}");
        assert_eq_diagnostic!(ssa.matches("abort @source(").count(), 1, "{ssa}");
        let llvm = render_verified_program(&program).expect("assertion SSA verifies before LLVM");
        assert_diagnostic!(!llvm.contains("call ptr @malloc"), "{llvm}");
        assert_diagnostic!(!ssa.contains("rc.retain"), "{ssa}");
        // Preserve the legacy spelling above, but actually forbid the current retain opcode.
        assert_diagnostic!(
            program
                .modules
                .iter()
                .all(|module| module.functions.iter().all(|function| function
                    .instructions
                    .iter()
                    .all(|instruction| !matches!(
                        instruction.operation,
                        Operation::SharedRetain { .. }
                    )))),
            "owned assertion must not retain: {ssa}"
        );
    }
}
}
