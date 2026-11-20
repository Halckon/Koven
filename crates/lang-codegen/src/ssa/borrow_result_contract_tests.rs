//! 正常源码产物及纯 verifier 合同负例；不改写或注入 LLVM/运行时故障。
use super::{
    LoweringErrorKind, borrow_memory_test_support::single as memory_unit, model::*,
    verify::verify_program, verify_borrow_result::call_contract,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, resolve_names},
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

const PROVIDER: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)\nfun consume(own source: String) { println(source) }\n";
const ENTRY: &str = "fun entry() { val source = \"kept\".clone(); block { borrow val item = wrap(source); println((item)) }; consume(source) }";

// The main-branch Legacy-only negative control requires its actual Single frontend.
pub(super) fn single(text: &str) -> Result<Program, super::LoweringError> {
    let mut sources = SourceMap::new();
    let source = sources.add_source("result.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    super::lower_frontend::orchestrate::lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
}

fn assert_flow(program: &Program) {
    verify_program(program).unwrap();
    let module = &program.modules[0];
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("entry"))
        .unwrap();
    let (result, source, call) = entry
        .instructions
        .iter()
        .find_map(|instruction| {
            if let Operation::BorrowCall { source, .. } = instruction.operation {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    panic!("result must be loan");
                };
                Some((*result, source, instruction.id.index()))
            } else {
                None
            }
        })
        .unwrap();
    assert!(entry.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::PrintString { value } if value == result)), "caller must use returned pointer");
    let result_end = entry.instructions.iter().position(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == result)).unwrap();
    let source_end = entry.instructions.iter().position(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == source)).unwrap();
    let delivery = entry
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .unwrap();
    assert!(call < result_end && result_end < source_end && source_end < delivery);
    for function in module
        .functions
        .iter()
        .filter(|function| function.borrow_return.is_some())
    {
        assert!(function.blocks.iter().any(|block| matches!(
            block.terminator.as_ref().unwrap().kind,
            TerminatorKind::BorrowReturn { .. }
        )));
        assert!(!function.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::Drop { .. }
                | Operation::Copy { .. }
                | Operation::StringClone { .. }
                | Operation::SharedRetain { .. }
        )));
    }
}

#[test]
fn abandoned_borrow_result_has_no_result_operation_or_callee_instance() {
    let source = r#"fun view(source: String, own n: Int): borrow String from source = source
fun entry() {
    val owner = "kept".clone()
    borrow val item = view(owner, return)
    println(item)
}"#;
    for lower in [memory_unit, super::borrow_storage_contract_tests::unit] {
        let program = lower(source).unwrap();
        verify_program(&program).unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        let mut functions = program.modules.iter().flat_map(|module| &module.functions);
        assert!(
            functions
                .clone()
                .all(|function| !function.name.contains(".view."))
        );
        let entry = functions
            .find(|function| function.name.contains(".entry."))
            .expect("entry lowered");
        assert!(
            entry.instructions.iter().any(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { .. })
            })
        );
        assert!(entry.instructions.iter().all(|instruction| {
            !matches!(
                instruction.operation,
                Operation::BorrowCall { .. } | Operation::PrintString { .. }
            )
        }));
    }
}

#[test]
fn borrowed_result_single_ssa_uses_returned_loan_and_orders_end_before_move() {
    assert_flow(&memory_unit(&format!("{PROVIDER}{ENTRY}")).unwrap());
}

#[test]
fn borrowed_result_unit_ssa_uses_returned_loan_and_orders_end_before_move() {
    let mut sources = SourceMap::new();
    let (p, provider) = super::unit_lower_test_support::parsed(
        &mut sources,
        "provider.ko",
        &format!("package p\n{PROVIDER}"),
    );
    let entry = ENTRY
        .replace("wrap(source)", "p.wrap(source)")
        .replace("consume(source)", "p.consume(source)");
    let (q, consumer) = super::unit_lower_test_support::parsed(
        &mut sources,
        "entry.ko",
        &format!("package q\n{entry}"),
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", p, &provider),
        SourceUnitInput::new("root", "q/entry.ko", q, &consumer),
    ];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let (program, _) = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        super::unit_lower_test_support::declaration(&names, "q", "entry"),
    )
    .unwrap();
    assert_flow(&program);
}

#[test]
fn borrowed_result_verifier_rejects_wrong_source_and_owned_result_contracts_without_ir_mutation() {
    let program = memory_unit(&format!("{PROVIDER}{ENTRY}")).unwrap();
    let module = &program.modules[0];
    let function = module
        .functions
        .iter()
        .find(|function| function.name.contains("entry"))
        .unwrap();
    let instruction = function
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
        .unwrap();
    let Operation::BorrowCall {
        callee,
        arguments,
        source,
    } = &instruction.operation
    else {
        unreachable!()
    };
    let [EntityId::Loan(result)] = instruction.results.as_slice() else {
        unreachable!()
    };
    let target = function
        .entity(EntityId::Loan(*result))
        .unwrap()
        .ty
        .semantic_type();
    let results = [EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    }];
    assert!(call_contract(
        module, function, *callee, arguments, *source, &results
    ));
    assert!(!call_contract(
        module, function, *callee, arguments, *result, &results
    ));
    assert!(!call_contract(
        module,
        function,
        *callee,
        arguments,
        *source,
        &[EntityType::Value(target)]
    ));
    verify_program(&program).unwrap();
}

#[test]
fn borrowed_result_source_move_stays_a_frontend_diagnostic_before_ssa() {
    use crate::{
        native_tests::boxed_enum_tests::unit_source::{SourceRejectionKind, UnitSource},
        test_support::diagnostics::assert_eq_diagnostic,
    };
    use lang_frontend::diagnostic::{Diagnostic, Severity, codes};

    let entry = "fun entry() { val source = \"kept\"; borrow val item = view(source); consume(source); println((item)) }";
    let text = format!("{PROVIDER}{entry}");
    let rejected = UnitSource::analyze("result.ko", &text)
        .err()
        .expect("moving a live borrowed source cannot acquire executable Unit capability");
    assert!(matches!(rejected.kind, SourceRejectionKind::Diagnostics));
    assert_eq!(
        rejected.report.stages,
        [("parse", 0), ("names", 0), ("types", 0), ("ownership", 1)]
    );
    let [(stage, diagnostic)] = rejected.report.diagnostics.as_slice() else {
        panic!("exactly the original source-move conflict");
    };
    assert_eq!(*stage, "ownership");
    let source = diagnostic.primary_span().source_id();
    assert_eq!(rejected.sources.source_name(source), Ok("result.ko"));
    assert_eq!(rejected.sources.source_text(source), Ok(text.as_str()));
    for (start, end, fragment) in [
        (255, 261, "source"),
        (238, 244, "source"),
        (140, 158, "own source: String"),
    ] {
        assert_eq!(&text[start..end], fragment);
    }
    // Frozen on 9369cc72: Unit keeps the original conflict and adds its selected parameter.
    // The full diagnostic includes every label in order, with no ignored notes or help.
    let mut expected = Diagnostic::new(
        &rejected.sources,
        Severity::Error,
        codes::catalog().unwrap().resolve("L0135").unwrap(),
        "move conflicts with an active loan",
        rejected.sources.span(source, 255, 261).unwrap(),
    )
    .unwrap();
    for (start, end, message) in [
        (238, 244, "conflicting loan starts here"),
        (140, 158, "selected parameter declared here"),
    ] {
        expected
            .add_label(
                &rejected.sources,
                rejected.sources.span(source, start, end).unwrap(),
                message,
            )
            .unwrap();
    }
    assert_eq_diagnostic!(
        rejected.report.diagnostics,
        [("ownership", expected)],
        "complete original conflict with Unit parameter context"
    );
}

#[test]
fn legacy_single_borrowed_result_caller_cfg_stays_unsupported() {
    let entry = "fun entry(flag: Boolean) { val source = \"kept\"; borrow val item = view(source); if (flag) { println((item)) }; println((item)) }";
    assert_eq!(
        single(&format!("{PROVIDER}{entry}")).err().unwrap().kind,
        LoweringErrorKind::UnsupportedNode
    );
}

// These exact sources formerly stopped at Unit lowering. Main now admits them;
// retain source/root/frontend identity and test the actual borrowed-result behavior.
const ORIGINAL_CALLER_ENTRY: &str = "fun entry(flag: Boolean) { val source = \"kept\"; borrow val item = view(source); if (flag) { println((item)) }; println((item)) }";
const ORIGINAL_BLOCK_RETURN: &str =
    "fun view(source: String): borrow String from source { return source }\nfun entry() {}";

fn lower_original_borrow_root(source: &str, root: &str) -> (Program, FunctionId) {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    let analysis = UnitSource::analyze("borrow-result-original.ko", source).unwrap();
    analysis.assert_frontend_clean();
    let declaration = analysis.declaration(root, SymbolKind::Function);
    let root_span = analysis.declaration_span(declaration);
    assert_eq!(
        analysis
            .sources()
            .source_name(root_span.source_id())
            .unwrap(),
        "borrow-result-original.ko"
    );
    assert_eq!(
        analysis
            .sources()
            .source_text(root_span.source_id())
            .unwrap(),
        source
    );
    let expected_root = if root == "view" {
        "fun view(source: String): borrow String from source { return source }"
    } else if source == ORIGINAL_BLOCK_RETURN {
        "fun entry() {}"
    } else {
        ORIGINAL_CALLER_ENTRY
    };
    assert_eq!(analysis.sources().slice(root_span).unwrap(), expected_root);
    if source == ORIGINAL_BLOCK_RETURN {
        let [origin] = analysis.ownership().borrow_return_origins() else {
            panic!("one actual block-view borrow-result declaration");
        };
        let marker = origin.declaration_span();
        assert_eq!(marker.source_id(), root_span.source_id());
        assert_eq!(marker.start(), source.find("borrow").unwrap());
        assert_eq!(marker.end(), marker.start() + "borrow".len());
        assert_eq!(analysis.sources().slice(marker).unwrap(), "borrow");
    } else {
        let [binding] = analysis.ownership().borrow_results().bindings() else {
            panic!("one actual caller result binding");
        };
        assert_eq!(binding.marker_span().source_id(), root_span.source_id());
        assert!(binding.source_loan().is_some());
        assert!(binding.parent().is_none());
    }
    let (program, entry) = analysis.lower_entry(declaration).unwrap();
    assert_eq!(program.modules.len(), 1);
    let function = program.modules[0]
        .function(entry)
        .expect("actual selected root");
    assert_eq!(function.origin.span(), root_span);
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    (program, entry)
}

fn assert_original_identity_borrow_return(function: &Function) {
    assert_eq!(function.borrow_return, Some(0));
    let (source, target) =
        super::verify_borrow_result::parameter(function).expect("actual shared source parameter");
    let returns: Vec<_> = function
        .blocks
        .iter()
        .filter_map(|block| match block.terminator.as_ref().unwrap().kind {
            TerminatorKind::BorrowReturn { loan } => Some(loan),
            _ => None,
        })
        .collect();
    assert_eq!(
        returns,
        [source],
        "identity body returns its original parameter loan"
    );
    assert_eq!(
        function.entity(EntityId::Loan(source)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target
        }
    );
    assert!(
        function.instructions.iter().all(|instruction| !matches!(
            instruction.operation,
            Operation::Drop { .. }
                | Operation::Copy { .. }
                | Operation::StringClone { .. }
                | Operation::SharedRetain { .. }
                | Operation::BorrowEnd { .. }
        )),
        "borrow provider does not consume, copy, retain or end its source"
    );
}

// This original caller has only an acyclic If. Follow positional SSA edge
// transport through *every* predecessor; matching loan types is not identity.
fn original_caller_transports_loan(function: &Function, loan: LoanId, anchor: LoanId) -> bool {
    fn visit(
        function: &Function,
        loan: LoanId,
        anchor: LoanId,
        visiting: &mut std::collections::BTreeSet<LoanId>,
    ) -> bool {
        if loan == anchor {
            return true;
        }
        if !visiting.insert(loan) {
            return false; // No cyclic or self-supporting provenance proof.
        }
        let parameter = function.blocks.iter().find_map(|block| {
            block
                .parameters
                .iter()
                .position(|entity| *entity == EntityId::Loan(loan))
                .map(|index| (block.id, index))
        });
        let Some((target, index)) = parameter else {
            visiting.remove(&loan);
            return false;
        };
        let mut incoming = Vec::new();
        for block in &function.blocks {
            let edges: Vec<&Edge> = match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                TerminatorKind::Return { .. } => Vec::new(),
                _ => return false, // Do not generalize beyond this source's CFG.
            };
            for edge in edges {
                if edge.target == target {
                    let Some(EntityId::Loan(argument)) = edge.arguments.get(index) else {
                        return false;
                    };
                    incoming.push(*argument);
                }
            }
        }
        let valid = !incoming.is_empty()
            && incoming
                .into_iter()
                .all(|argument| visit(function, argument, anchor, visiting));
        visiting.remove(&loan);
        valid
    }
    visit(
        function,
        loan,
        anchor,
        &mut std::collections::BTreeSet::new(),
    )
}

#[test]
fn original_borrowed_result_caller_cfg_preserves_live_result_and_end_order() {
    let source = format!("{PROVIDER}{ORIGINAL_CALLER_ENTRY}");
    let (program, entry) = lower_original_borrow_root(&source, "entry");
    let module = &program.modules[0];
    assert_eq!(
        module.functions.len(),
        2,
        "only entry and its actual view callee"
    );
    let function = module.function(entry).unwrap();
    let calls: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
        .collect();
    let [call] = calls.as_slice() else {
        panic!("one borrow-result acquisition")
    };
    let Operation::BorrowCall { callee, source, .. } = call.operation else {
        unreachable!()
    };
    let [EntityId::Loan(result)] = call.results.as_slice() else {
        panic!("one result loan")
    };
    assert_original_identity_borrow_return(module.function(callee).unwrap());
    let mut prints: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::PrintString { .. }))
        .collect();
    prints.sort_by_key(|print| print.origin.span().start());
    assert_eq!(prints.len(), 2);
    let branches: Vec<_> = function
        .blocks
        .iter()
        .filter(|block| {
            matches!(
                block.terminator.as_ref().unwrap().kind,
                TerminatorKind::Conditional { .. }
            )
        })
        .collect();
    let [branch] = branches.as_slice() else {
        panic!("the original If CFG")
    };
    let dominators = super::verify::compute_dominators(function);
    let dominates = |a: BlockId, b: BlockId| dominators[b.index()].contains(&a.index());
    assert!(
        prints
            .iter()
            .all(|print| dominates(call.block, print.block))
    );
    let (branch_print, joined_print) = (prints[0], prints[1]);
    assert!(branch_print.origin.span().start() < joined_print.origin.span().start());
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &branch.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    assert!(dominates(when_true.target, branch_print.block));
    assert!(!dominates(when_false.target, branch_print.block));
    assert_ne!(branch_print.block, joined_print.block);
    assert!(
        !dominates(branch_print.block, joined_print.block),
        "conditional read cannot dominate the joined read"
    );
    assert!(dominates(branch.id, branch_print.block));
    assert!(dominates(branch.id, joined_print.block));
    let end = |loan| {
        let ends: Vec<_> = function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { loan: actual }
                if original_caller_transports_loan(function, actual, loan))
            })
            .collect();
        assert_eq!(
            ends.len(),
            1,
            "each original loan ends exactly once after CFG transport"
        );
        ends[0]
    };
    let result_end = end(*result);
    let source_end = end(source);
    let before = |a: &Instruction, b: &Instruction| {
        assert!(dominates(a.block, b.block));
        if a.block == b.block {
            assert!(a.id.index() < b.id.index());
        }
    };
    assert_ne!(result_end.id, source_end.id);
    for print in &prints {
        let Operation::PrintString { value } = print.operation else {
            unreachable!()
        };
        assert!(
            original_caller_transports_loan(function, value, *result),
            "both reads must receive the original returned loan on every incoming edge"
        );
        assert!(
            !original_caller_transports_loan(function, value, source),
            "the source loan cannot substitute for the returned result"
        );
    }
    before(joined_print, result_end);
    before(result_end, source_end);
    let returns: Vec<_> = function.blocks.iter().filter(|block|
        matches!(&block.terminator.as_ref().unwrap().kind, TerminatorKind::Return { values } if values.is_empty())).collect();
    assert_eq!(returns.len(), 1);
    assert!(dominates(source_end.block, returns[0].id));
}

#[test]
fn original_uncalled_block_borrow_return_keeps_selected_empty_entry() {
    let (program, entry) = lower_original_borrow_root(ORIGINAL_BLOCK_RETURN, "entry");
    let module = &program.modules[0];
    assert_eq!(
        module.functions.len(),
        1,
        "uncalled view is not an execution witness"
    );
    let function = module.function(entry).unwrap();
    assert_eq!(function.borrow_return, None);
    assert!(function.instructions.is_empty());
    let [block] = function.blocks.as_slice() else {
        panic!("empty entry has one block")
    };
    assert!(matches!(&block.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.is_empty()));
}

#[test]
fn original_block_borrow_return_root_returns_the_source_loan() {
    let (program, entry) = lower_original_borrow_root(ORIGINAL_BLOCK_RETURN, "view");
    let module = &program.modules[0];
    assert_eq!(module.functions.len(), 1);
    assert_original_identity_borrow_return(module.function(entry).unwrap());
}

const LOCAL_RESULT_CONTROL: &str = "if (flag) { println(\"branch\") }";
const LOCAL_RESULT_BINDING: &str = "borrow val item = view(source)";
const LOCAL_RESULT_READ: &str = "println(item)";

fn local_result_control_sources(body: &str) -> [String; 3] {
    let text = format!(
        "{PROVIDER}fun probe(flag: Boolean): Unit {{\n    val source = \"kept\".clone()\n    {body}\n}}\nfun entry() {{ probe(true); probe(false) }}"
    );
    let with_sibling =
        format!("{text}\nfun unrelated(): Unit {{ var a = 1; var b = 2; swap(&a, &b) }}");
    let with_lambda = text.replace(
        "\n}\nfun entry()",
        "\n    val unrelated: () -> Unit = { var a = 1; var b = 2; swap(&a, &b) }\n}\nfun entry()",
    );
    assert_ne!(text, with_lambda);
    [text, with_sibling, with_lambda]
}

fn assert_disjoint_local_result_control(body: &str, branch_before_borrow: bool) {
    for (variant, source) in local_result_control_sources(body).iter().enumerate() {
        for lower in [memory_unit, super::borrow_storage_contract_tests::unit] {
            let program =
                lower(source).unwrap_or_else(|error| panic!("layout {variant}: {body}: {error:?}"));
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
            assert_local_result_order(&program, branch_before_borrow);
        }
    }
}

fn assert_local_result_order(program: &Program, branch_before_borrow: bool) {
    let probe = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains(".probe."))
        .expect("probe lowered");
    let branch = probe
        .blocks
        .iter()
        .find(|block| {
            block.terminator.as_ref().is_some_and(|terminator| {
                matches!(terminator.kind, TerminatorKind::Conditional { .. })
            })
        })
        .expect("actual If branch");
    let call = probe
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
        .expect("actual ordinary borrow acquisition");
    let dominators = super::verify::compute_dominators(probe);
    let dominates =
        |first: BlockId, second: BlockId| dominators[second.index()].contains(&first.index());
    if branch_before_borrow {
        assert_ne!(
            branch.id, call.block,
            "acquisition cannot precede the branch terminator"
        );
        assert!(dominates(branch.id, call.block));
    } else {
        let Operation::BorrowCall { source, .. } = call.operation else {
            unreachable!()
        };
        let [EntityId::Loan(result)] = call.results.as_slice() else {
            panic!("ordinary call returns one loan");
        };
        let end = |loan| {
            probe
                .instructions
                .iter()
                .find(|instruction| instruction.operation == Operation::BorrowEnd { loan })
                .expect("loan ends before leaving its lexical scope")
        };
        let result_end = end(*result);
        let source_end = end(source);
        assert!(dominates(result_end.block, source_end.block));
        if result_end.block == source_end.block {
            assert!(result_end.id.index() < source_end.id.index());
        }
        assert!(dominates(source_end.block, branch.id));
    }
}

#[test]
fn ordinary_result_local_accepts_control_before_acquisition() {
    for body in [
        format!("{LOCAL_RESULT_CONTROL}\n    {LOCAL_RESULT_BINDING}\n    {LOCAL_RESULT_READ}"),
        format!(
            "{LOCAL_RESULT_CONTROL}\n    block {{ {LOCAL_RESULT_BINDING}; {LOCAL_RESULT_READ} }}"
        ),
        format!(
            "block {{ {LOCAL_RESULT_CONTROL} }}\n    {LOCAL_RESULT_BINDING}\n    {LOCAL_RESULT_READ}"
        ),
    ] {
        assert_disjoint_local_result_control(&body, true);
    }
}

#[test]
fn ordinary_result_local_accepts_control_after_lexical_end() {
    for following in [
        LOCAL_RESULT_CONTROL.to_owned(),
        format!("block {{ {LOCAL_RESULT_CONTROL} }}"),
    ] {
        assert_disjoint_local_result_control(
            &format!("block {{ {LOCAL_RESULT_BINDING}; {LOCAL_RESULT_READ} }}\n    {following}"),
            false,
        );
    }
}

#[test]
fn ordinary_result_local_preserves_control_while_scope_still_owns_the_loan() {
    for body in [
        format!("{LOCAL_RESULT_BINDING}\n    {LOCAL_RESULT_CONTROL}\n    {LOCAL_RESULT_READ}"),
        format!("{LOCAL_RESULT_BINDING}\n    {LOCAL_RESULT_READ}\n    {LOCAL_RESULT_CONTROL}"),
    ] {
        for source in local_result_control_sources(&body) {
            for lower in [memory_unit, super::borrow_storage_contract_tests::unit] {
                let program = lower(&source).unwrap_or_else(|error| panic!("{body}: {error:?}"));
                verify_program(&program).unwrap();
                crate::llvm::render_verified_program(&program).unwrap();
            }
        }
    }
}

fn with_extra_value_parameter(source: &str) -> String {
    source
        .replacen(
            "fun view(source: String)",
            "fun view(source: String, own extra: Int)",
            1,
        )
        .replacen(
            "= view(source)\nfun consume",
            "= view(source, 1)\nfun consume",
            1,
        )
}

#[test]
fn ordinary_result_local_ends_source_before_dropping_owner_on_argument_return() {
    let body = format!(
        "borrow val item = view(source, if (flag) {{ return }} else {{ 1 }})\n    {LOCAL_RESULT_READ}"
    );
    for source in local_result_control_sources(&body) {
        let source = with_extra_value_parameter(&source);
        for lower in [memory_unit, super::borrow_storage_contract_tests::unit] {
            let program = lower(&source).unwrap();
            verify_program(&program).unwrap();
            crate::llvm::render_verified_program(&program).unwrap();
            let probe = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.contains(".probe."))
                .unwrap();
            let (return_edge, normal_edge) = probe
                .blocks
                .iter()
                .find_map(|block| {
                    let TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } = &block.terminator.as_ref()?.kind
                    else {
                        return None;
                    };
                    Some((when_true, when_false))
                })
                .expect("argument conditional");
            assert_eq!(return_edge.arguments, normal_edge.arguments);
            let returning = &probe.blocks[return_edge.target.index()];
            let normal = &probe.blocks[normal_edge.target.index()];
            let [EntityId::Value(owner), EntityId::Loan(returning_loan)] =
                returning.parameters.as_slice()
            else {
                panic!("exit arm receives owner and source loan");
            };
            let [EntityId::Value(_), EntityId::Loan(normal_loan)] = normal.parameters.as_slice()
            else {
                panic!("normal arm receives owner and source loan");
            };
            assert!(matches!(
                returning.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ));
            let dispatches: Vec<_> = probe
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::BorrowCall { .. }))
                .collect();
            let [dispatch] = dispatches.as_slice() else {
                panic!("only the normal arm dispatches");
            };
            assert_eq!(dispatch.block, normal.id);
            assert!(
                matches!(dispatch.operation, Operation::BorrowCall { source, .. } if source == *normal_loan)
            );
            let mut exit = returning
                .instructions
                .iter()
                .map(|id| &probe.instructions[id.index()]);
            assert!(exit.any(|i| i.operation
                == Operation::BorrowEnd {
                    loan: *returning_loan
                }));
            assert!(exit.any(|i| i.operation == Operation::Drop { owner: *owner }));
        }
    }
}

#[test]
fn ordinary_result_argument_return_shares_only_proved_disjoint_controls() {
    let conditional =
        "borrow val item = view(source, if (flag) { return } else { 1 }); println(item)";
    let disjoint = format!(
        "block {{ borrow val before = view(source, 0); println(before) }}\n    block {{ {conditional} }}\n    block {{ borrow val after = view(source, 2); println(after) }}"
    );
    let overlapping = format!(
        "borrow val before = view(source, 0)\n    block {{ {conditional} }}\n    println(before)"
    );
    let missing_prefix = format!(
        "block {{ {conditional} }}\n    block {{ borrow val unproved = view(extra = if (flag) {{ return }} else {{ 1 }}, source = source); println(unproved) }}"
    );
    for (body, accepted) in [
        (disjoint, true),
        (overlapping, false),
        (missing_prefix, false),
    ] {
        for source in local_result_control_sources(&body) {
            let source = with_extra_value_parameter(&source);
            for (lower, prefix_len) in [
                (memory_unit as fn(&str) -> _, 0),
                (
                    super::borrow_storage_contract_tests::unit,
                    "package p\n".len(),
                ),
            ] {
                if accepted {
                    let program = lower(&source).unwrap();
                    verify_program(&program).unwrap();
                    crate::llvm::render_verified_program(&program).unwrap();
                } else {
                    let error = lower(&source)
                        .err()
                        .expect("sibling has no applicable control proof");
                    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
                    let span = error.span.expect("exact unsupported control boundary");
                    let start = source.rfind("return").unwrap() + prefix_len;
                    assert_eq!((span.start(), span.end()), (start, start + "return".len()));
                }
            }
        }
    }
}

#[test]
fn ordinary_result_local_preserves_argument_boundary() {
    for (arguments, rejected) in [
        (
            "source, if (flag) { 1 } else { 2 }",
            "if (flag) { 1 } else { 2 }",
        ),
        ("source, -1", "-1"),
        (
            "extra = if (flag) { return } else { 1 }, source = source",
            "return",
        ),
    ] {
        let body = format!("borrow val item = view({arguments})\n    {LOCAL_RESULT_READ}");
        for source in local_result_control_sources(&body) {
            let source = with_extra_value_parameter(&source);
            for (lower, prefix_len) in [
                (memory_unit as fn(&str) -> _, 0),
                (
                    super::borrow_storage_contract_tests::unit,
                    "package p\n".len(),
                ),
            ] {
                let error = lower(&source)
                    .err()
                    .expect("borrowed call argument rejected");
                assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
                let span = error.span.expect("exact argument boundary");
                let start = source.find(rejected).unwrap() + prefix_len;
                assert_eq!((span.start(), span.end()), (start, start + rejected.len()));
            }
        }
    }
}
