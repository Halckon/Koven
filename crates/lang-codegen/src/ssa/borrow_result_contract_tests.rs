//! 正常源码产物及纯 verifier 合同负例；不改写或注入 LLVM/运行时故障。
use super::{
    LoweringErrorKind, model::*, verify::verify_program, verify_borrow_result::call_contract,
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
    assert_flow(&single(&format!("{PROVIDER}{ENTRY}")).unwrap());
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
    let program = single(&format!("{PROVIDER}{ENTRY}")).unwrap();
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
