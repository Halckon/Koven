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

#[test]
fn while_break_and_continue_drop_body_local_owners_before_their_edges() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun sink(own input: String): Unit {}\n\
         fun route(own flag: Boolean, own text: String): Unit {\n\
             while (flag) {\n\
                 val breakLocal = \"break\"\n\
                 val continueLocal = \"continue\"\n\
                 if (flag) {\n\
                     val moved = sink(breakLocal)\n\
                     continue\n\
                 } else {\n\
                     val moved = sink(continueLocal)\n\
                     break\n\
                 }\n\
             }\n\
             val finalValue = sink(text)\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val done = p.route(false, \"outer\") }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    assert_eq!(
        owned
            .ownership()
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.point(), UnitDropPoint::ControlTransfer(_))
                    && matches!(fact.target(), UnitDropTarget::Named(_))
                    && sources
                        .slice(fact.value_origin())
                        .is_ok_and(|name| matches!(name, "breakLocal" | "continueLocal"))
            })
            .count(),
        2
    );

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("break and continue lower after cleaning their body-local owner");
    let route = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.route.d"))
        .expect("route function exists");
    let header = match &route
        .blocks
        .first()
        .and_then(|block| block.terminator.as_ref())
        .expect("route entry branches to while header")
        .kind
    {
        TerminatorKind::Branch(edge) => edge.target,
        _ => panic!("route entry must be the while preheader"),
    };
    assert!(route.blocks.iter().any(|block| matches!(
        block.terminator.as_ref().map(|terminator| &terminator.kind),
        Some(TerminatorKind::Branch(edge))
            if block.id != route.entry_block().expect("entry exists")
                && edge.target == header
                && edge.arguments.len() == 2
    )));
    assert_eq!(
        program.modules[0]
            .functions
            .iter()
            .flat_map(|function| function.instructions.iter())
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        3,
        "both jumps drop one local and sink owns all delivered String values"
    );
}

#[test]
fn bare_loop_break_reaches_the_following_function_exit_without_a_fake_condition() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun stop(own text: String): Unit { loop { break } }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val done = p.stop(\"stop\") }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("bare loop break lowers to a verified exit");
    let stop = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.stop.d"))
        .expect("stop function exists");
    assert_eq!(
        stop.blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .filter(|terminator| matches!(terminator.kind, TerminatorKind::Conditional { .. }))
            .count(),
        0
    );
    assert_eq!(
        stop.blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .filter(|terminator| matches!(terminator.kind, TerminatorKind::Branch(_)))
            .count(),
        1
    );
}

#[test]
fn bare_loop_all_break_exits_may_consume_the_outer_owner() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun sink(own input: String): Unit {}\n\
         fun consume(own text: String): Unit {\n\
             loop {\n\
                 val done = sink(text)\n\
                 break\n\
             }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val done = p.consume(\"owned\") }",
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
    .expect("coarse LoopExit fact is skipped when every real exit consumed the owner");
    let consume = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.consume.d"))
        .expect("consume function exists");
    assert_eq!(
        consume
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        0,
        "consume transfers text to sink and must not synthesize a stale LoopExit drop"
    );
}

#[test]
fn nested_continue_targets_the_inner_while_header() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun nested(own outer: Boolean, own inner: Boolean): Unit {\n\
             while (outer) {\n\
                 while (inner) {\n\
                     if (inner) { continue } else { break }\n\
                 }\n\
                 break\n\
             }\n\
         }\n\
         fun entry(): Unit { val done = nested(false, false) }",
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
    .expect("nested loop jumps target the nearest context");
    let nested = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.nested.d"))
        .expect("nested function exists");
    let entry = nested.entry_block().expect("nested entry exists");
    let outer_header = match &nested
        .block(entry)
        .and_then(|block| block.terminator.as_ref())
        .expect("entry branches to outer header")
        .kind
    {
        TerminatorKind::Branch(edge) => edge.target,
        _ => panic!("nested entry must be a preheader"),
    };
    let conditional_blocks = nested
        .blocks
        .iter()
        .filter(|block| {
            matches!(
                block.terminator.as_ref().map(|terminator| &terminator.kind),
                Some(TerminatorKind::Conditional { .. })
            )
        })
        .map(|block| block.id)
        .collect::<Vec<_>>();
    assert!(conditional_blocks.len() >= 3);
    let inner_targets = nested
        .blocks
        .iter()
        .filter(|block| block.id != entry)
        .filter_map(|block| match &block.terminator.as_ref()?.kind {
            TerminatorKind::Branch(edge)
                if edge.target != outer_header && conditional_blocks.contains(&edge.target) =>
            {
                Some(edge.target)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(inner_targets.iter().any(|target| {
        inner_targets
            .iter()
            .filter(|candidate| *candidate == target)
            .count()
            >= 2
    }));
}

#[test]
fn nested_break_merges_with_the_inner_false_exit_before_outer_return() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun nested(own outer: Boolean, own inner: Boolean): Unit {\n\
             while (outer) {\n\
                 while (inner) { break }\n\
                 return\n\
             }\n\
         }\n\
         fun entry(): Unit { val done = nested(false, false) }",
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
    .expect("inner break and false exit merge before the outer body continues");
    let nested = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.nested.d"))
        .expect("nested function exists");
    let entry = nested.entry_block().expect("nested entry exists");
    let outer_header = match &nested
        .block(entry)
        .and_then(|block| block.terminator.as_ref())
        .expect("entry branches to outer header")
        .kind
    {
        TerminatorKind::Branch(edge) => edge.target,
        _ => panic!("nested entry must be a preheader"),
    };
    let (_, inner_true, inner_false) = nested
        .blocks
        .iter()
        .find_map(|block| match &block.terminator.as_ref()?.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } if block.id != outer_header => Some((block.id, when_true, when_false)),
            _ => None,
        })
        .expect("inner while header exists");
    let true_merge = match &nested
        .block(inner_true.target)
        .and_then(|block| block.terminator.as_ref())
        .expect("inner break edge exists")
        .kind
    {
        TerminatorKind::Branch(edge) => edge.target,
        _ => panic!("inner break must branch to the inner exit merge"),
    };
    let false_merge = match &nested
        .block(inner_false.target)
        .and_then(|block| block.terminator.as_ref())
        .expect("inner false edge exists")
        .kind
    {
        TerminatorKind::Branch(edge) => edge.target,
        _ => panic!("inner false exit must branch to the inner exit merge"),
    };
    assert_eq!(true_merge, false_merge);
    assert!(matches!(
        nested
            .block(true_merge)
            .and_then(|block| block.terminator.as_ref())
            .map(|terminator| &terminator.kind),
        Some(TerminatorKind::Return { .. })
    ));
}
