//! Existing frontend loans retain the outer source-call prefix through inner loops.
//! Both cases require a real sealed named source and clean frontend gates before lowering.
use crate::ssa::{
    model::{EntityId, EntityType, Operation, SsaTypeKind},
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{UnitCallableOrigin, UnitLoanTarget},
    parser::Expression,
    source::SourceMap,
    type_checking::{UnitCallableTarget, UnitExpressionId, UnitTypeKind, standard_environments},
};

fn check_existing_frontend_loan_survives_inner_loop(control: &str) {
    let text = format!(
        "package test\nfun identity(index: Int): Int = index\nfun helper(callback: (Int)->Int, count: Int): Int {{ val items = List<Int>(count, callback)\nreturn items[2] }}\nfun entry(): Int {{ val outer = Array<Int>(1, {{ index -> index }})\nfor (value in outer) {{ return helper(identity, if (true) {{ {control}\n3 }} else {{ 3 }}) }}\nreturn 0 }}"
    );
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "test/abi-slot-loop.ko", &text);
    let inputs = [SourceUnitInput::new(
        "root",
        "test/abi-slot-loop.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let identities = file
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Expression::Name)
                && &text[node.span().start()..node.span().end()] == "identity"
        })
        .collect::<Vec<_>>();
    assert_eq!(identities.len(), 1);
    let initializer = UnitExpressionId::new(
        names
            .names()
            .references()
            .iter()
            .find(|reference| reference.span() == identities[0].1.span())
            .expect("resolved source identity")
            .source_unit(),
        identities[0].0,
    );
    assert!(matches!(
        typed
            .types()
            .expression_type(initializer)
            .and_then(|ty| typed.types().types().get(ty)),
        Some(UnitTypeKind::Function { .. })
    ));
    assert_eq!(
        owned
            .ownership()
            .callable_origin(initializer)
            .expect("real sealed identity")
            .origin(),
        UnitCallableOrigin::KnownFunction(UnitCallableTarget::Declaration(declaration(
            &names, "test", "identity"
        )))
    );
    assert!(
        owned
            .ownership()
            .loans()
            .iter()
            .any(|loan| loan.argument() == initializer
                && matches!(loan.target(), UnitLoanTarget::Temporary(owner) if *owner == initializer)),
        "this source-call operand follows its real frontend Temporary LoanFact"
    );
    assert_eq!(
        typed.types().sequential_iterations().len(),
        1,
        "real for facts authorize this existing control-prefix capability"
    );
    let before = typed.types().types().len();
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("an inner loop control transfer must retain the existing outer call loan");
    let module = &program.modules[0];
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("test.entry"))
        .expect("entry body");
    let addresses = entry
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation, Operation::FunctionAddress { target }
            if module.functions[target.index()].name.contains("test.identity"))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        addresses.len(),
        1,
        "the selected named source address is formed once"
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .count(),
        1,
        "the callback address remains live until the normal helper call"
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| {
                let Operation::Drop { owner } = instruction.operation else {
                    return false;
                };
                let Some(EntityType::Value(ty)) =
                    entry.entity(EntityId::Value(owner)).map(|e| e.ty)
                else {
                    return false;
                };
                matches!(
                    module.types.get(ty.index()),
                    Some(SsaTypeKind::FunctionPointer { .. })
                )
            })
            .count(),
        2,
        "both the outer generation lambda and the source-call address owner are cleaned; verifier proves each owner is consumed exactly once"
    );
    crate::llvm::render_verified_program(&program)
        .expect("existing frontend loan reaches verified LLVM");
    assert_eq!(typed.types().types().len(), before);
}

#[test]
fn unit_runtime_existing_frontend_loan_survives_inner_loop_break() {
    check_existing_frontend_loan_survives_inner_loop("while (true) { break }");
}

#[test]
fn unit_runtime_existing_frontend_loan_survives_inner_loop_continue() {
    check_existing_frontend_loan_survives_inner_loop(
        "var step = 0\nwhile (step < 1) { step += 1\ncontinue }",
    );
}
