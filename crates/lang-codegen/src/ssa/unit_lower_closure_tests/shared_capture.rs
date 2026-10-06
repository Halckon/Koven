//! A nonescaping borrowed closure now consumes the published shared-capture contract.
use super::*;
use lang_frontend::{
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode as FrontendCaptureMode},
    parser::Expression,
    type_checking::UnitExpressionId,
};

#[test]
fn borrowed_closure_keeps_its_source_owner_until_the_closure_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/borrowed.ko",
        "package test\n\
         fun inspect(message: String): Unit {}\n\
         fun entry(): Unit {\n\
             val message = \"borrowed\"\n\
             val action: () -> Unit = { -> val read = inspect(message) }\n\
             val invoked = action()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/borrowed.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let lambda = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| matches!(node.payload(), Expression::Lambda { .. }))
        .expect("real source lambda")
        .0;
    let lambda = UnitExpressionId::new(names.names().source_units()[0].source_unit(), lambda);
    let captures = owned.ownership().captures_of(lambda).collect::<Vec<_>>();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].mode(), FrontendCaptureMode::Shared);
    assert_eq!(captures[0].effect(), ClosureCaptureEffect::Borrow);
    let arena = typed.types().types().len();
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("guide permits a borrowed closure's synchronous call in its defining callable");
    let module = &program.modules[0];
    let entry = function(module, "test.entry");
    let owner = entry
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
        .expect("captured source owner")
        .results[0];
    let closure = entry
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::ClosureConstruct { .. }))
        .expect("shared capture environment")
        .results[0];
    let drop_index = |entity| {
        entry
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.operation, Operation::Drop { owner }
                if EntityId::Value(owner) == entity)
            })
            .expect("one real owner drop")
    };
    assert_eq!(operation_count(entry, is_callable_invoke), 1);
    assert_eq!(operation_count(entry, is_drop), 2);
    assert!(drop_index(closure) < drop_index(owner));
    let thunk = module
        .functions
        .iter()
        .find(|function| function.name.contains(".thunk"))
        .expect("borrowed closure thunk");
    assert!(thunk.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::SharedReferenceFollow { .. }
    )));
    crate::llvm::render_verified_program(&program)
        .expect("shared capture lifetime reaches verified LLVM without consuming the source");
    assert_eq!(typed.types().types().len(), arena);
}
