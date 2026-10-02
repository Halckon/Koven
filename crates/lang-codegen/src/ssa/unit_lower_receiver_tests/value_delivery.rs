use super::*;

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

#[test]
fn interface_value_default_drops_move_only_concrete_receiver_once() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Finishable { own fun finish(): Int = 40 }\n\
         class Resource: Finishable {}\n\
         fun entry(): Int = Resource().finish()",
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
    .expect("MoveOnly StaticSelf specialization must consume its conditional receiver drop");

    let module = &program.modules[0];
    let finish = function(module.functions.iter(), ".Finishable.finish.s");
    assert_eq!(
        finish
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "callee owns and drops the concrete Resource exactly once",
    );
    let entry = function(module.functions.iter(), ".entry.d");
    assert!(
        !entry
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
    render_verified_program(&program).expect("MoveOnly interface Value default must lower to LLVM");
}

#[test]
fn interface_value_default_skips_drop_for_copyable_concrete_receiver() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Finishable {\n\
             own fun finish(): Int = 40\n\
             own fun relay(): Int = finish()\n\
         }\n\
         value class Counter(val item: Int): Finishable {}\n\
         fun entry(): Int {\n\
             val counter = Counter(1)\n\
             val first = counter.relay()\n\
             return counter.relay() + first\n\
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
    .expect("Copyable StaticSelf specialization must skip its conditional receiver drop");

    let module = &program.modules[0];
    let finish = function(module.functions.iter(), ".Finishable.finish.s");
    assert!(
        !finish
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
    let relay = function(module.functions.iter(), ".Finishable.relay.s");
    assert!(relay.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::DirectCall {
            receiver: Some(EntityId::Value(_)),
            ..
        }
    )));
    assert!(
        !relay
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );
    render_verified_program(&program).expect("Copyable interface Value default must lower to LLVM");
}

#[test]
fn interface_value_defaults_deliver_static_self_through_explicit_and_implicit_calls() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Parent { own fun finish(): Int = 40 }\n\
         interface Finishable: Parent {\n\
             own fun terminal(own item: Int): Int = item\n\
             own fun relay(): Int = terminal(40)\n\
             own fun forward(): Int = this.relay()\n\
             own fun inherited(): Int = this.finish()\n\
             own fun qualified(): Int = super<Parent>.finish()\n\
         }\n\
         class Resource: Finishable {}\n\
         fun entry(): Int = Resource().forward() + Resource().inherited() + Resource().qualified()",
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
    .expect("conditional StaticSelf deliveries must lower without a synthetic receiver fact");

    let module = &program.modules[0];
    let finish = function(module.functions.iter(), ".Finishable.terminal.s");
    let relay = function(module.functions.iter(), ".Finishable.relay.s");
    let forward = function(module.functions.iter(), ".Finishable.forward.s");
    let inherited = function(module.functions.iter(), ".Finishable.inherited.s");
    let qualified = function(module.functions.iter(), ".Finishable.qualified.s");
    assert_eq!(
        finish
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "the terminal Value receiver owner is dropped exactly once",
    );
    let parent_finish = function(module.functions.iter(), ".Parent.finish.s");
    assert_eq!(
        parent_finish
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
    );
    for caller in [relay, forward, inherited, qualified] {
        let arguments = caller.instructions.iter().find_map(|instruction| {
            let Operation::DirectCall {
                receiver: Some(EntityId::Value(_)),
                arguments,
                ..
            } = &instruction.operation
            else {
                return None;
            };
            Some(arguments)
        });
        let arguments = arguments.expect("Value receiver must be the zeroth call operand");
        if caller.name.contains(".relay.") {
            assert_eq!(arguments.len(), 1, "explicit arguments follow the receiver");
        }
        assert!(
            !caller
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.operation, Operation::Drop { .. })),
            "an intermediate owner is delivered, not dropped",
        );
    }
    render_verified_program(&program)
        .expect("explicit and implicit StaticSelf Value deliveries must lower to LLVM");
}

#[test]
fn interface_value_default_delivers_receiver_on_each_early_return_edge() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Finishable {\n\
             own fun terminal(): Int = 1\n\
             own fun finish(flag: Boolean): Int {\n\
                 if (flag) { return terminal() }\n\
                 return terminal()\n\
             }\n\
         }\n\
         class Resource: Finishable {}\n\
         fun entry(): Int = Resource().finish(true)",
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
    .expect("conditional receiver drops must cover every reachable return edge");

    let finish = function(program.modules[0].functions.iter(), ".Finishable.finish.s");
    assert!(
        !finish
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. })),
        "each return edge transfers the receiver instead of dropping it",
    );
    assert_eq!(
        finish
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::DirectCall {
                    receiver: Some(EntityId::Value(_)),
                    ..
                }
            ))
            .count(),
        2,
    );
    let terminal = function(
        program.modules[0].functions.iter(),
        ".Finishable.terminal.s",
    );
    assert_eq!(
        terminal
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "the selected terminal callee owns the receiver",
    );
    render_verified_program(&program).expect("early-return receiver drops must lower to LLVM");
}

#[test]
fn interface_value_default_merges_delivered_and_retained_receiver_paths() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Finishable {\n\
             own fun terminal(): Unit {}\n\
             own fun maybe(flag: Boolean): Int {\n\
                 if (flag) { val delivered = terminal() }\n\
                 return 2\n\
             }\n\
             own fun both(flag: Boolean): Int {\n\
                 if (flag) { val left = terminal() }\n\
                 else { val right = terminal() }\n\
                 return 3\n\
             }\n\
         }\n\
         class Resource: Finishable {}\n\
         fun entry(): Int = Resource().maybe(false) + Resource().both(true)",
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
    .expect("the retained path must drop while the delivered path keeps no receiver owner");

    render_verified_program(&program)
        .expect("asymmetric conditional receiver ownership must lower to verified LLVM");
}

#[test]
fn consumed_receiver_identity_survives_divergent_sibling_lowering() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Finishable {\n\
             own fun terminal(): Unit {}\n\
             own fun finish(outer: Boolean, inner: Boolean): Int {\n\
                 val delivered = terminal()\n\
                 if (outer) {\n\
                     if (inner) { return 1 } else { return 2 }\n\
                 } else {}\n\
                 return 3\n\
             }\n\
         }\n\
         class Resource: Finishable {}\n\
         fun entry(): Int = Resource().finish(false, false)",
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
    .expect("a divergent sibling must not erase the already-delivered receiver identity");

    let finish = function(program.modules[0].functions.iter(), ".Finishable.finish.s");
    assert!(
        !finish
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. })),
        "the receiver was delivered before the nested control split",
    );
    render_verified_program(&program)
        .expect("consumed receiver identity must survive nested divergent control flow");
}

#[test]
fn consumed_receiver_identity_survives_divergent_while_body_lowering() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Finishable {\n\
             own fun terminal(): Unit {}\n\
             own fun finish(repeat: Boolean, inner: Boolean): Int {\n\
                 val delivered = terminal()\n\
                 while (repeat) {\n\
                     if (inner) { return 1 } else { return 2 }\n\
                 }\n\
                 return 3\n\
             }\n\
         }\n\
         class Resource: Finishable {}\n\
         fun entry(): Int = Resource().finish(false, false)",
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
    .expect("a divergent while body must not erase the false-edge receiver identity");

    render_verified_program(&program)
        .expect("consumed receiver identity must survive a divergent while body");
}
