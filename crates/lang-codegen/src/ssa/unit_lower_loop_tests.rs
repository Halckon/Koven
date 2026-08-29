use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    source::SourceMap,
    type_checking::standard_environments,
};

use super::{
    model::{Operation, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn jump_free_while_carries_a_string_owner_on_a_deterministic_backedge() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun sink(own input: String): Unit {}\n\
         fun spin(own flag: Boolean, own text: String): Unit {\n\
             while (flag) {\n\
                 if (flag) {} else {\n\
                     val done = sink(text)\n\
                     return\n\
                 }\n\
             }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val done = p.spin(false, \"carried\") }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    assert!(owned.ownership().drops().iter().any(|fact| {
        matches!(fact.point(), UnitDropPoint::LoopExit(_))
            && matches!(fact.target(), UnitDropTarget::Named(_))
            && sources
                .slice(fact.value_origin())
                .is_ok_and(|name| name == "text")
    }));
    let entry = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("jump-free while lowers to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("source permutation preserves while SSA");
    assert_eq!(render_program(&forward), render_program(&backward));

    let spin = forward.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.spin.d"))
        .expect("spin function exists");
    let (header, when_true, when_false) = spin
        .blocks
        .iter()
        .find_map(|block| match &block.terminator.as_ref()?.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => Some((block.id, when_true, when_false)),
            _ => None,
        })
        .expect("while has one conditional header");
    assert_eq!(
        spin.block(header).expect("header exists").parameters.len(),
        2
    );
    assert_eq!(when_true.arguments.len(), 2);
    let entry_block = spin.entry_block().expect("spin has an entry block");
    assert!(spin.blocks.iter().any(|block| matches!(
        block.terminator.as_ref().map(|terminator| &terminator.kind),
        Some(TerminatorKind::Branch(edge))
            if block.id != entry_block && edge.target == header && edge.arguments.len() == 2
    )));
    assert!(matches!(
        spin.block(when_false.target)
            .and_then(|block| block.terminator.as_ref())
            .map(|terminator| &terminator.kind),
        Some(TerminatorKind::Return { .. })
    ));
    assert_eq!(
        forward.modules[0]
            .functions
            .iter()
            .flat_map(|function| function.instructions.iter())
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        2,
        "the return path transfers to sink and the outer false path consumes LoopExit"
    );
}

#[test]
fn while_zero_iteration_exit_drops_the_owner_not_returned_by_the_body() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun sink(own input: String): Unit {}\n\
         fun route(own flag: Boolean, own text: String): Unit {\n\
             while (flag) {\n\
                 val done = sink(text)\n\
                 return\n\
             }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val done = p.route(false, \"exit\") }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    assert!(owned.ownership().drops().iter().any(|fact| {
        matches!(fact.point(), UnitDropPoint::LoopExit(_))
            && matches!(fact.target(), UnitDropTarget::Named(_))
            && sources
                .slice(fact.value_origin())
                .is_ok_and(|name| name == "text")
    }));

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("while return and zero-iteration exit discharge the owner once per path");
    assert_eq!(
        program.modules[0]
            .functions
            .iter()
            .flat_map(|function| function.instructions.iter())
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        2,
        "sink drops on the true path and LoopExit drops on the false path"
    );
}
