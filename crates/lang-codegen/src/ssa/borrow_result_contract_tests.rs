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
const ENTRY: &str = "fun entry() { val source = \"kept\".clone(); { borrow val item = wrap(source); println((item)) }; consume(source) }";

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
    let entry = "fun entry() { val source = \"kept\"; borrow val item = view(source); consume(source); println((item)) }";
    assert_eq!(
        single(&format!("{PROVIDER}{entry}")).err().unwrap().kind,
        LoweringErrorKind::FrontendDiagnostics
    );
}

#[test]
fn borrowed_result_caller_cfg_and_block_returns_stay_precisely_unsupported() {
    let entry = "fun entry(flag: Boolean) { val source = \"kept\"; borrow val item = view(source); if (flag) { println((item)) }; println((item)) }";
    assert_eq!(
        single(&format!("{PROVIDER}{entry}")).err().unwrap().kind,
        LoweringErrorKind::UnsupportedNode
    );
    let block =
        "fun view(source: String): borrow String from source { return source }\nfun entry() {}";
    assert_eq!(
        single(block).err().unwrap().kind,
        LoweringErrorKind::UnsupportedNode
    );
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
        format!("{LOCAL_RESULT_CONTROL}\n    {{ {LOCAL_RESULT_BINDING}; {LOCAL_RESULT_READ} }}"),
        format!(
            "{{ {LOCAL_RESULT_CONTROL} }}\n    {LOCAL_RESULT_BINDING}\n    {LOCAL_RESULT_READ}"
        ),
    ] {
        assert_disjoint_local_result_control(&body, true);
    }
}

#[test]
fn ordinary_result_local_accepts_control_after_lexical_end() {
    for following in [
        LOCAL_RESULT_CONTROL.to_owned(),
        format!("{{ {LOCAL_RESULT_CONTROL} }}"),
    ] {
        assert_disjoint_local_result_control(
            &format!("{{ {LOCAL_RESULT_BINDING}; {LOCAL_RESULT_READ} }}\n    {following}"),
            false,
        );
    }
}

#[test]
fn ordinary_result_local_rejects_control_while_scope_still_owns_the_loan() {
    for body in [
        format!("{LOCAL_RESULT_BINDING}\n    {LOCAL_RESULT_CONTROL}\n    {LOCAL_RESULT_READ}"),
        format!("{LOCAL_RESULT_BINDING}\n    {LOCAL_RESULT_READ}\n    {LOCAL_RESULT_CONTROL}"),
    ] {
        for (variant, source) in local_result_control_sources(&body).iter().enumerate() {
            for (lower, prefix_len) in [
                (memory_unit as fn(&str) -> _, 0),
                (
                    super::borrow_storage_contract_tests::unit,
                    "package p\n".len(),
                ),
            ] {
                let error = lower(source).err().unwrap_or_else(|| {
                    panic!("layout {variant}: the result loan has not reached its end: {body}")
                });
                assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
                let span = error.span.expect("control expression span");
                let start = source.find(LOCAL_RESULT_CONTROL).unwrap() + prefix_len;
                assert_eq!(
                    (span.start(), span.end()),
                    (start, start + LOCAL_RESULT_CONTROL.len())
                );
            }
        }
    }
}

#[test]
fn ordinary_result_local_preserves_argument_boundary() {
    for argument in ["if (flag) { 1 } else { 2 }", "-1"] {
        let body = format!("borrow val item = view(source, {argument})\n    {LOCAL_RESULT_READ}");
        for source in local_result_control_sources(&body) {
            let source = source
                .replacen(
                    "fun view(source: String)",
                    "fun view(source: String, own extra: Int)",
                    1,
                )
                .replacen(
                    "= view(source)\nfun consume",
                    "= view(source, 1)\nfun consume",
                    1,
                );
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
                let start = source.find(argument).unwrap() + prefix_len;
                assert_eq!((span.start(), span.end()), (start, start + argument.len()));
            }
        }
    }
}
