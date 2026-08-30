use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{EntityId, EntityType, Function, LoanKind, Operation},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use crate::llvm::render_verified_program;

#[test]
fn lowers_borrow_member_receiver_before_explicit_arguments() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         value class Counter(val item: Int) {\n\
             fun answer(own delta: Int): Int = item + delta\n\
         }\n\
         fun entry(): Int = Counter(1).answer(40)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Borrow member call must lower to verified SSA");

    let module = &program.modules[0];
    let member = function(module.functions.iter(), ".Counter.answer.s");
    assert!(matches!(
        member.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        })
    ));
    assert!(matches!(member.blocks[0].parameters[0], EntityId::Loan(_)));
    assert!(
        member
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::SharedFieldLoan { .. }))
    );

    let entry = function(module.functions.iter(), ".entry.d");
    let operations = entry
        .instructions
        .iter()
        .map(|instruction| &instruction.operation)
        .collect::<Vec<_>>();
    let call = operations
        .iter()
        .position(|operation| matches!(operation, Operation::DirectCall { .. }))
        .expect("member direct call");
    let borrow = operations
        .iter()
        .position(|operation| matches!(operation, Operation::BorrowBegin { .. }))
        .expect("receiver borrow begins");
    let argument = operations[..call]
        .iter()
        .rposition(|operation| matches!(operation, Operation::Constant(_)))
        .expect("explicit argument is evaluated");
    assert!(matches!(
        operations[borrow - 1],
        Operation::RootPlace { .. }
    ));
    assert!(matches!(
        operations[borrow],
        Operation::BorrowBegin {
            kind: LoanKind::Shared,
            ..
        }
    ));
    assert!(borrow < argument && argument < call);
    assert!(matches!(
        operations[call],
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(_)),
            arguments,
            ..
        } if matches!(arguments.as_slice(), [EntityId::Value(_)])
    ));
    assert!(matches!(operations[call + 1], Operation::BorrowEnd { .. }));

    let rendered = render_program(&program);
    assert!(rendered.contains("receiver %l0"), "{rendered}");
    let llvm = render_verified_program(&program).expect("Borrow receiver must lower to LLVM");
    assert!(llvm.contains("Counter.answer"), "{llvm}");
    assert!(llvm.contains("ptr %l0, i32 %v0"), "{llvm}");
    assert!(llvm.contains("(ptr %p0, i32 40)"), "{llvm}");
}

#[test]
fn forwards_implicit_this_loan_without_readdressing_receiver() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         value class Counter(val item: Int) {\n\
             fun answer(own delta: Int): Int = item + delta\n\
             fun relay(own delta: Int): Int = answer(delta)\n\
         }\n\
         fun entry(): Int {\n\
             val counter = Counter(1)\n\
             return counter.relay(40)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("stable and implicit Borrow receivers must lower");

    let module = &program.modules[0];
    let relay = function(module.functions.iter(), ".Counter.relay.s");
    let EntityId::Loan(this_loan) = relay.blocks[0].parameters[0] else {
        panic!("relay receiver must be a shared loan");
    };
    assert!(!relay.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::RootPlace { .. } | Operation::BorrowBegin { .. }
    )));
    assert!(relay.instructions.iter().any(|instruction| matches!(
        &instruction.operation,
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(receiver)),
            arguments,
            ..
        } if *receiver == this_loan && matches!(arguments.as_slice(), [EntityId::Value(_)])
    )));

    let entry = function(module.functions.iter(), ".entry.d");
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
            .count(),
        1
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
            .count(),
        1
    );
    let llvm = render_verified_program(&program).expect("forwarded this loan must lower to LLVM");
    assert!(llvm.contains("Counter.relay"), "{llvm}");
    assert!(llvm.contains("Counter.answer"), "{llvm}");
}

#[test]
fn forwards_explicit_borrow_binding_as_member_receiver() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource { fun answer(): Int = 41 }\n\
         fun relay(resource: Resource): Int = resource.answer()\n\
         fun entry(): Int {\n\
             val resource = Resource()\n\
             return relay(resource)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Borrow parameter loan must be reusable as a Borrow member receiver");

    let relay = function(program.modules[0].functions.iter(), ".relay.d");
    let EntityId::Loan(parameter) = relay.blocks[0].parameters[0] else {
        panic!("relay parameter must be a shared loan");
    };
    assert!(relay.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(receiver)),
            ..
        } if receiver == parameter
    )));
    assert!(!relay.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::RootPlace { .. } | Operation::BorrowBegin { .. }
    )));
    render_verified_program(&program).expect("forwarded Borrow parameter must lower to LLVM");
}

#[test]
fn value_this_can_borrow_for_an_implicit_member_call() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             fun answer(): Int = 41\n\
             own fun relay(): Int = answer()\n\
         }\n\
         fun entry(): Int = Resource().relay()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Value this must create a call-scoped shared loan for Borrow member calls");

    let relay = function(program.modules[0].functions.iter(), ".Resource.relay.s");
    assert!(relay.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::BorrowBegin {
            kind: LoanKind::Shared,
            ..
        }
    )));
    assert!(
        relay
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
    );
    assert!(
        relay
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
    render_verified_program(&program).expect("Value-to-Borrow receiver loan must lower to LLVM");
}

#[test]
fn inout_this_reborrows_shared_for_an_implicit_member_call() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             fun answer(): Int = 41\n\
             inout fun relay(): Int = answer()\n\
         }\n\
         fun entry(): Int {\n\
             var resource = Resource()\n\
             return resource.relay()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Inout this must create a call-scoped shared reborrow");

    let relay = function(program.modules[0].functions.iter(), ".Resource.relay.s");
    assert!(
        relay
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::SharedReborrow { .. }))
    );
    assert!(
        relay
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
    );
    render_verified_program(&program).expect("exclusive-to-shared reborrow must lower to LLVM");
}

#[test]
fn borrow_class_receiver_preserves_owner_until_post_call_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource(val id: Int) {\n\
             fun answer(): Int = 41\n\
         }\n\
         fun entry(): Int {\n\
             val resource = Resource(1)\n\
             return resource.answer()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Borrow class receiver must preserve and later drop its owner");

    let entry = function(program.modules[0].functions.iter(), ".entry.d");
    let call = entry
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
        .expect("member call");
    let borrow_end = entry
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
        .expect("receiver loan end");
    let drop = entry
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .expect("class owner drop");
    assert!(call < borrow_end && borrow_end < drop);
    let llvm = render_verified_program(&program).expect("Borrow class receiver must lower to LLVM");
    assert!(llvm.contains("Resource.answer"), "{llvm}");
    assert!(llvm.contains("call void @free"), "{llvm}");
}

#[test]
fn inout_class_receiver_uses_exclusive_call_scoped_loan() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             inout fun touch(): Unit {}\n\
         }\n\
         fun entry(): Unit {\n\
             var resource = Resource()\n\
             val result = resource.touch()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Inout class receiver must lower as an exclusive call-scoped loan");

    let module = &program.modules[0];
    let touch = function(module.functions.iter(), ".Resource.touch.s");
    assert!(matches!(
        touch.receiver(),
        Some(EntityType::Loan {
            kind: LoanKind::Exclusive,
            ..
        })
    ));

    let entry = function(module.functions.iter(), ".entry.d");
    let operations = entry
        .instructions
        .iter()
        .map(|instruction| &instruction.operation)
        .collect::<Vec<_>>();
    let call = operations
        .iter()
        .position(|operation| matches!(operation, Operation::DirectCall { .. }))
        .expect("member call");
    assert!(matches!(
        operations[call - 1],
        Operation::BorrowBegin {
            kind: LoanKind::Exclusive,
            ..
        }
    ));
    assert!(matches!(
        operations[call],
        Operation::DirectCall {
            receiver: Some(EntityId::Loan(_)),
            ..
        }
    ));
    assert!(matches!(operations[call + 1], Operation::BorrowEnd { .. }));
    assert!(
        operations[call + 2..]
            .iter()
            .any(|operation| matches!(operation, Operation::Drop { .. }))
    );

    let llvm = render_verified_program(&program).expect("Inout receiver must lower to LLVM");
    assert!(llvm.contains("Resource.touch"), "{llvm}");
}

#[test]
fn value_receiver_reuses_copyable_inline_value() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         value class Counter(val item: Int) {\n\
             own fun add(own delta: Int): Int = item + delta\n\
         }\n\
         fun entry(): Int {\n\
             val counter = Counter(1)\n\
             val first = counter.add(1)\n\
             return counter.add(first)\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Value receiver must preserve a Copyable inline value");

    let module = &program.modules[0];
    let add = function(module.functions.iter(), ".Counter.add.s");
    assert!(matches!(add.receiver(), Some(EntityType::Value(_))));
    assert!(
        add.instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::AggregateProject { .. }))
    );

    let entry = function(module.functions.iter(), ".entry.d");
    let calls = entry
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                receiver: Some(receiver),
                ..
            } => Some(*receiver),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    assert!(matches!(calls[0], EntityId::Value(_)));
    assert_eq!(calls[0], calls[1], "Copyable receiver remains reusable");
    render_verified_program(&program).expect("Value receiver ABI must lower to LLVM");
}

#[test]
fn value_receiver_moves_class_owner_to_callee_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource(val id: Int) {\n\
             own fun finish(): Int = 40\n\
         }\n\
         fun entry(): Int {\n\
             val resource = Resource(1)\n\
             return resource.finish()\n\
         }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("MoveOnly class receiver must transfer to the callee");

    let module = &program.modules[0];
    let finish = function(module.functions.iter(), ".Resource.finish.s");
    assert!(matches!(finish.receiver(), Some(EntityType::Value(_))));
    assert_eq!(
        finish
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "callee owns and drops the moved class receiver"
    );
    let entry = function(module.functions.iter(), ".entry.d");
    assert!(entry.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            receiver: Some(EntityId::Value(_)),
            ..
        }
    )));
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        0,
        "moved class receiver has no caller-side drop"
    );
    render_verified_program(&program).expect("moved Value receiver must lower to LLVM");
}

#[test]
fn value_receiver_can_return_this_without_callee_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             own fun pass(): Resource = this\n\
         }\n\
         fun entry(): Resource = Resource().pass()",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("return this must transfer the Value receiver exactly once");

    let pass = function(program.modules[0].functions.iter(), ".Resource.pass.s");
    assert!(matches!(pass.receiver(), Some(EntityType::Value(_))));
    assert!(
        !pass
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
    render_verified_program(&program).expect("returned receiver owner must lower to LLVM");
}

#[test]
fn value_receiver_is_carried_and_dropped_on_each_conditional_exit() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             own fun choose(flag: Boolean): Int = if (flag) 1 else 2\n\
         }\n\
         fun entry(): Int = Resource().choose(true)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Value receiver must remain linear across conditional CFG edges");

    let choose = function(program.modules[0].functions.iter(), ".Resource.choose.s");
    assert_eq!(
        choose
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "both normal branches merge the still-owned receiver before its control-transfer drop"
    );
    render_verified_program(&program).expect("conditional receiver drops must lower to LLVM");
}

#[test]
fn value_receiver_is_rebound_across_while_edges() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         class Resource {\n\
             own fun wait(flag: Boolean): Int {\n\
                 while (flag) {}\n\
                 return 1\n\
             }\n\
         }\n\
         fun entry(): Int = Resource().wait(false)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Value receiver identity must be rebound on loop header, body and false edges");

    let wait = function(program.modules[0].functions.iter(), ".Resource.wait.s");
    assert!(wait.blocks.len() >= 4);
    assert_eq!(
        wait.instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1
    );
    render_verified_program(&program).expect("loop-carried receiver must lower to LLVM");
}

fn function<'a>(
    mut functions: impl Iterator<Item = &'a Function>,
    name_fragment: &str,
) -> &'a Function {
    functions
        .find(|function| function.name.contains(name_fragment))
        .expect("planned function exists")
}
