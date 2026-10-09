//! 查询未改写的普通源码 SSA，校验通用 storage 和实际投影的 shared 合同。
use super::{borrow_result_contract_tests::single, model::*, verify::verify_program};

#[test]
fn borrowed_last_use_direct_place_alias_preserves_parent_and_permission_restore() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(own source: String) {}\nfun entry() { val source = \"kept\"; borrow val parent = view(source); borrow val alias = parent; println(alias); consume(source) }";
    for result in [single(text), unit(text)] {
        let program = result.unwrap();
        assert_alias_dependencies(&program);
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
fn borrowed_direct_alias_single_and_unit_preserve_real_parent_and_cleanup_order() {
    for case in crate::native_borrow_last_use_cases::alias_cases() {
        for (label, result) in [("single", single(case.source)), ("unit", unit(case.source))] {
            let program = result.unwrap_or_else(|error| panic!("{}/{label}: {error:?}", case.name));
            assert_alias_dependencies(&program);
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}

#[test]
fn borrowed_stable_place_single_and_unit_use_real_storage_and_ordered_ends() {
    for case in crate::native_borrow_place_cases::cases() {
        for (label, result) in [("single", single(case.source)), ("unit", unit(case.source))] {
            let program = result.unwrap_or_else(|error| panic!("{}/{label}: {error:?}", case.name));
            assert_alias_dependencies(&program);
            let entry = program.modules[0]
                .functions
                .iter()
                .find(|f| f.name.contains("entry"))
                .unwrap();
            assert!(!entry.instructions.iter().any(|i| matches!(
                i.operation,
                Operation::Copy { .. } | Operation::SharedRetain { .. }
            )));
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}

fn assert_alias_dependencies(program: &Program) {
    verify_program(program).unwrap();
    let entry = program.modules[0]
        .functions
        .iter()
        .find(|f| f.name.contains("entry"))
        .unwrap();
    let end = |loan| {
        let ends = entry.instructions.iter().enumerate().filter_map(|(i, instruction)|
            matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan).then_some(i)).collect::<Vec<_>>();
        assert_eq!(ends.len(), 1, "each real loan must end exactly once");
        ends[0]
    };
    let mut aliases = 0;
    for (index, instruction) in entry.instructions.iter().enumerate() {
        let source = match instruction.operation {
            Operation::SharedReborrow { source } => {
                aliases += 1;
                source
            }
            Operation::BorrowCall { source, .. } | Operation::MapRequireValue { source, .. } => {
                source
            }
            Operation::SharedFieldLoan { base, .. }
            | Operation::SharedHeapFieldLoan { base, .. } => base,
            _ => continue,
        };
        let [EntityId::Loan(child)] = instruction.results.as_slice() else {
            panic!("real child loan")
        };
        assert_ne!(*child, source);
        assert!(index < end(*child) && end(*child) < end(source));
        if matches!(instruction.operation, Operation::SharedReborrow { .. }) {
            assert_eq!(
                entry.entity(EntityId::Loan(*child)).unwrap().ty,
                entry.entity(EntityId::Loan(source)).unwrap().ty
            );
        }
        for (i, operation) in entry.instructions.iter().enumerate().skip(index + 1) {
            if matches!(operation.operation, Operation::MapPut { .. })
                || matches!(&operation.operation, Operation::DirectCall { arguments, .. } if arguments.iter().any(|a| matches!(a, EntityId::Value(_))))
            {
                assert!(
                    end(source) < i,
                    "source lease ends before owned delivery/mutation"
                );
            }
        }
    }
    assert!(aliases > 0);
}

#[test]
fn borrowed_last_use_single_and_unit_preserve_real_dependencies_and_cleanup_order() {
    for case in crate::native_borrow_last_use_cases::cases() {
        for (label, program) in [("single", single(case.source)), ("unit", unit(case.source))] {
            let program =
                program.unwrap_or_else(|error| panic!("{}/{label}: {error:?}", case.name));
            verify_program(&program).unwrap();
            let module = &program.modules[0];
            let entry = module
                .functions
                .iter()
                .find(|f| f.name.contains("entry"))
                .unwrap();
            let end = |loan| {
                let ends = entry.instructions.iter().enumerate().filter_map(|(i, instruction)|
                    matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan).then_some(i)).collect::<Vec<_>>();
                assert_eq!(ends.len(), 1, "{}/{label}: one real loan end", case.name);
                ends[0]
            };
            let mut calls = Vec::new();
            for (index, instruction) in entry.instructions.iter().enumerate() {
                let source = match instruction.operation {
                    Operation::BorrowCall { source, .. }
                    | Operation::MapRequireValue { source, .. } => source,
                    _ => continue,
                };
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    panic!("shared result")
                };
                assert!(index < end(*result) && end(*result) < end(source));
                assert_eq!(
                    entry.entity(EntityId::Loan(*result)).unwrap().ty,
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: entry
                            .entity(EntityId::Loan(*result))
                            .unwrap()
                            .ty
                            .semantic_type()
                    }
                );
                calls.push((source, *result));
                if matches!(instruction.operation, Operation::MapRequireValue { .. }) {
                    let later_mutations = entry
                        .instructions
                        .iter()
                        .enumerate()
                        .filter(|(i, operation)| {
                            *i > index && matches!(operation.operation, Operation::MapPut { .. })
                        })
                        .collect::<Vec<_>>();
                    assert!(
                        later_mutations.iter().all(|(i, _)| end(source) < *i),
                        "slot must end before later Map mutation"
                    );
                }
            }
            assert!(!calls.is_empty());
            if case.name == "last-use-chain" {
                assert_eq!(calls.len(), 3);
                assert_eq!(calls[1].0, calls[0].1);
                assert_eq!(calls[2].0, calls[1].1);
                let print = entry.instructions.iter().position(|i|
                    matches!(i.operation, Operation::PrintString { value } if value == calls[2].1)).unwrap();
                let consume = entry
                    .instructions
                    .iter()
                    .position(|i| matches!(i.operation, Operation::DirectCall { .. }))
                    .unwrap();
                assert!(print < end(calls[2].1) && end(calls[0].0) < consume);
            }
            for function in module
                .functions
                .iter()
                .filter(|f| f.borrow_return.is_some())
            {
                assert!(!function.instructions.iter().any(|i| matches!(
                    i.operation,
                    Operation::Drop { .. }
                        | Operation::Copy { .. }
                        | Operation::SharedRetain { .. }
                        | Operation::StringClone { .. }
                )));
            }
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}

#[test]
fn borrowed_storage_single_generic_and_projection_preserve_delivery_and_source_contracts() {
    for callable in ["view", "wrap"] {
        for (name, text, _) in crate::native_borrow_storage_cases::cases(callable) {
            let program = single(&text).unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert_storage_contracts(name, &program);
        }
    }
}

#[test]
fn borrowed_storage_unit_generic_and_projection_preserve_delivery_and_source_contracts() {
    for callable in ["view", "wrap"] {
        for (name, text, _) in crate::native_borrow_storage_cases::cases(callable) {
            let program = unit(&text).unwrap_or_else(|error| panic!("{name}: {error:?}"));
            assert_storage_contracts(name, &program);
        }
    }
}

fn assert_storage_contracts(name: &str, program: &Program) {
    verify_program(program).unwrap();
    let module = &program.modules[0];
    let llvm = crate::llvm::render_verified_program(program).unwrap();
    let mut returns = 0;
    for function in &module.functions {
        if function.borrow_return.is_none() {
            continue;
        }
        returns += 1;
        let (source, target) = super::verify_borrow_result::parameter(function).unwrap();
        assert!(
            !function.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::Drop { .. }
                    | Operation::Copy { .. }
                    | Operation::StringClone { .. }
                    | Operation::SharedRetain { .. }
            )),
            "borrowed callable must not copy or drop its source"
        );
        let block = function
            .blocks
            .iter()
            .find(|block| {
                matches!(
                    block.terminator.as_ref().unwrap().kind,
                    TerminatorKind::BorrowReturn { .. }
                )
            })
            .unwrap();
        let terminator = block.terminator.as_ref().unwrap();
        let TerminatorKind::BorrowReturn { loan } = terminator.kind else {
            unreachable!()
        };
        assert_eq!(
            function.entity(EntityId::Loan(loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target
            }
        );
        let llvm_name = format!("f{}.{}", function.id.index(), function.name);
        let header = llvm
            .lines()
            .find(|line| line.starts_with("define internal ptr ") && line.contains(&llvm_name))
            .unwrap();
        let body = llvm
            .split_once(header)
            .unwrap()
            .1
            .split("\n}")
            .next()
            .unwrap();
        assert!(
            !body.contains("ret ptr null"),
            "nullable payload null is not a missing loan"
        );
        if loan == source {
            assert!(body.contains(&format!("ret ptr %l{}", source.index())));
            assert!(
                !body.contains("load "),
                "identity return must return storage, not payload"
            );
        }
        if name == "nullable-storage" {
            assert!(
                !body.contains("load "),
                "nullable forwarding must not unwrap its payload"
            );
        }
        if name == "projected-storage" {
            assert_ne!(
                function
                    .entity(EntityId::Loan(source))
                    .unwrap()
                    .ty
                    .semantic_type(),
                target
            );
            let mut errors = Vec::new();
            super::verify_borrow_result::return_contract(
                function,
                source,
                block.id,
                &terminator.origin,
                &mut errors,
            );
            assert!(
                !errors.is_empty(),
                "source root must not masquerade as field payload"
            );
        }
    }
    assert!(returns > 0);
    for function in &module.functions {
        for instruction in &function.instructions {
            if let Operation::BorrowCall {
                callee,
                arguments,
                source,
            } = &instruction.operation
            {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    unreachable!()
                };
                let result_ty = function.entity(EntityId::Loan(*result)).unwrap().ty;
                assert!(super::verify_borrow_result::call_contract(
                    module,
                    function,
                    *callee,
                    arguments,
                    *source,
                    &[result_ty]
                ));
                assert!(!super::verify_borrow_result::call_contract(
                    module,
                    function,
                    *callee,
                    arguments,
                    *result,
                    &[result_ty]
                ));
                assert!(!super::verify_borrow_result::call_contract(
                    module,
                    function,
                    *callee,
                    arguments,
                    *source,
                    &[EntityType::Value(result_ty.semantic_type())]
                ));
            }
        }
    }
    if name == "nullable-storage" {
        assert!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.instructions)
                .any(|i| matches!(i.operation, Operation::NullableNull { .. }))
        );
        assert!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.instructions)
                .any(|i| matches!(i.operation, Operation::NullableWrap { .. }))
        );
        assert!(
            module
                .functions
                .iter()
                .filter(|f| f.borrow_return.is_some())
                .all(|f| matches!(
                    module.type_kind(f.return_types[0]),
                    Some(SsaTypeKind::NullableHandle { .. })
                ))
        );
    }
}

pub(super) fn unit(text: &str) -> Result<Program, super::LoweringError> {
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };
    let (provider, entry) = text.split_once("fun entry()").unwrap();
    let mut sources = SourceMap::new();
    let (p, provider) = super::unit_lower_test_support::parsed(
        &mut sources,
        "provider.ko",
        &format!("package p\n{provider}"),
    );
    let (q, consumer) = super::unit_lower_test_support::parsed(
        &mut sources,
        "entry.ko",
        &format!("package q\nimport p.*\nfun entry(){entry}"),
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", p, &provider),
        SourceUnitInput::new("root", "q/entry.ko", q, &consumer),
    ];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        super::unit_lower_test_support::declaration(&names, "q", "entry"),
    )
    .map(|(program, _)| program)
}

#[test]
fn borrowed_storage_single_and_unit_ordinary_nullable_comparison_uses_real_presence() {
    let text = "class Token(val n: Int)\nfun consume(own source: Token?) { if (source == null) { println(\"null\") } else { println(\"present\") } }\nfun entry() { val source: Token? = null; consume(source) }";
    for (label, program) in [("single", single(text)), ("unit", unit(text))] {
        let program = program.unwrap_or_else(|error| panic!("{label}: {error:?}"));
        verify_program(&program).unwrap();
        let functions = &program.modules[0].functions;
        assert!(functions.iter().any(|f| {
            f.instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::NullableIsNull { .. }))
                || f.blocks.iter().any(|b| {
                    matches!(
                        b.terminator.as_ref().unwrap().kind,
                        TerminatorKind::NullableBranch { .. }
                    )
                })
        }));
        assert!(functions.iter().any(|f| {
            f.instructions.iter().any(|i| {
                let Operation::Drop { owner } = i.operation else {
                    return false;
                };
                let target = f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type();
                matches!(
                    program.modules[0].type_kind(target),
                    Some(SsaTypeKind::NullableHandle { .. })
                )
            })
        }));
        let llvm = crate::llvm::render_verified_program(&program).unwrap();
        assert!(llvm.contains("icmp eq ptr"));
    }
}
