//! Stage A: eleven unchanged legacy source instances, eighteen independent Unit roots.
//! Legacy tests remain intact. These selected-root SSA/LLVM oracles do not claim
//! whole-file, exact CFG, or native equivalence; admission failures are hard failures.
use super::{
    model::{
        EntityId, EntityType, LoanKind, Operation, Origin, Program, SsaTypeKind, TerminatorKind,
    },
    render::render_program,
    verify::verify_program,
};
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
    assert!(ssa.contains("nullable_handle<"), "{ssa}");
    assert!(ssa.contains("nullable.branch"), "{ssa}");
    assert!(ssa.contains("view %l"), "{ssa}");
    let llvm = render_verified_program(&program).expect("nullable smart cast LLVM");
    assert!(llvm.contains("icmp eq ptr"), "{llvm}");
    assert!(llvm.contains("phi ptr"), "{llvm}");
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
    let ssa = render_program(&program);
    assert!(ssa.contains("shared.retain %l"), "{ssa}");
    assert!(ssa.contains("shared.payload_place %l"), "{ssa}");
    assert!(ssa.contains("end_borrow"), "{ssa}");
    let llvm = render_verified_program(&program).expect("nullable Rc view LLVM");
    assert!(llvm.contains("icmp eq ptr"), "{llvm}");
    assert!(llvm.contains(".next = add i64"), "{llvm}");
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
        assert_eq!(ssa.matches("nullable.branch").count(), 2, "{ssa}");
        assert!(
            !ssa.contains("nullable.take"),
            "a view must not consume the owner: {ssa}"
        );
        assert!(
            !ssa.contains("shared.retain"),
            "read views must not share: {ssa}"
        );
        assert!(
            !ssa.contains("heap.allocate"),
            "proofs must not allocate: {ssa}"
        );
        let llvm = render_verified_program(&program)
            .expect("both nullable branches must pass LLVM lowering");
        assert!(!llvm.contains("call ptr @malloc"), "{llvm}");
        assert!(
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
        assert_eq!(ssa.matches("nullable.take").count(), 1, "{ssa}");
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
        assert_eq!(ssa.matches("nullable.take").count(), 1, "{ssa}");
        assert_eq!(ssa.matches("abort @source(").count(), 1, "{ssa}");
        let llvm = render_verified_program(&program).expect("assertion SSA verifies before LLVM");
        assert!(!llvm.contains("call ptr @malloc"), "{llvm}");
        assert!(!ssa.contains("rc.retain"), "{ssa}");
    }
}
