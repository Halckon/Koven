use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{EntityId, Operation, SsaTypeKind, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_cross_file_rc_retain_copyable_read_transfer_and_drop_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun seed(): Int = 40\n\
         fun build(): Rc<Int> = Rc(seed())\n\
         fun copied(own number: Int): Rc<Int> = Rc(number)\n\
         fun use(own owner: Rc<Int>): Int {\n\
             val retained = owner.share()\n\
             val copied = retained.value\n\
             return copied + owner.value\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.use(p.build()) + p.use(p.copied(p.seed()))",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
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
    .expect("cross-file Rc facts lower to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("input permutation preserves Rc lowering");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    assert_eq!(
        module
            .types
            .iter()
            .filter(|ty| matches!(ty, SsaTypeKind::SharedOwner { .. }))
            .count(),
        1,
        "Rc<Int> has one unit-global shared-owner identity"
    );
    let build = function(module, "p.build");
    let seed = function(module, "p.seed").id;
    let seed_result = build
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } if callee == seed => Some(instruction.results[0]),
            _ => None,
        })
        .expect("build calls seed");
    let allocated = build
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::SharedAllocate { payload, .. }
                if EntityId::Value(payload) == seed_result =>
            {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("seed result is delivered to Rc allocation");
    assert_eq!(returned(build), &[value(allocated)]);
    assert!(
        !build
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );

    let copied = function(module, "p.copied");
    let source = value(copied.blocks[0].parameters[0]);
    let independent = copied
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::Copy { source: actual } if actual == source => {
                Some(value(instruction.results[0]))
            }
            _ => None,
        })
        .expect("Copyable place construction creates an independent payload value");
    assert!(copied.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::SharedAllocate { payload, .. } if payload == independent
    )));

    let used = function(module, "p.use");
    let owner = used.blocks[0].parameters[0];
    let retained = used
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::SharedRetain { owner: actual } if actual == owner => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("share retains the original owner");
    let payload_owners = used
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::SharedPayloadPlace { owner } => Some(owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(payload_owners, vec![retained, owner]);
    let drops = used
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Drop { owner } => Some(owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(drops, vec![value(retained), value(owner)]);

    let entry_function = function(module, "q.entry");
    assert!(
        entry_function
            .instructions
            .iter()
            .all(|instruction| !matches!(instruction.operation, Operation::Drop { .. }))
    );
}

#[test]
fn moves_string_payload_into_rc_and_transfers_owner_across_calls() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun wrap(own text: String): Rc<String> = Rc(text)\n\
         fun discard(own owner: Rc<String>) {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry() { p.discard(p.wrap(\"owned\")) }",
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
    .expect("String payload and Rc owner are delivered exactly once");

    let module = &program.modules[0];
    let wrap = function(module, "p.wrap");
    let text = value(wrap.blocks[0].parameters[0]);
    let allocated = wrap
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::SharedAllocate { payload, .. } if payload == text => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("named String owner moves directly into Rc allocation");
    assert_eq!(returned(wrap), &[value(allocated)]);
    assert!(
        !wrap
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );

    let discard = function(module, "p.discard");
    let owner = value(discard.blocks[0].parameters[0]);
    assert_eq!(
        discard
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::Drop { owner } => Some(owner),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![owner]
    );
    let entry = function(module, "q.entry");
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        0,
        "String literal and returned Rc temporaries transfer into their callees"
    );
}

#[test]
fn rejects_move_only_payload_read_atomically() {
    let (name_environment, type_environment) = standard_environments();
    let mut sources = SourceMap::new();
    let (read_source, read) = parsed(
        &mut sources,
        "test/read.ko",
        "package test\nfun entry(): Boolean {\nval owner = Rc(\"owned\")\nreturn owner.value == \"owned\"\n}",
    );
    let read_inputs = [SourceUnitInput::new(
        "root",
        "test/read.ko",
        read_source,
        &read,
    )];
    let (names, typed, owned) =
        analyze(&sources, &read_inputs, &name_environment, &type_environment);
    let error = match lower_scalar_unit_with_entry(
        &sources,
        &read_inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    ) {
        Ok(_) => panic!("MoveOnly Rc payload read is not guessed into an owned value"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

fn function<'a>(module: &'a super::model::Module, name: &str) -> &'a super::model::Function {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .expect("reachable function exists")
}

fn returned(function: &super::model::Function) -> &[super::model::ValueId] {
    function
        .blocks
        .iter()
        .find_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Return { values }) => Some(values.as_slice()),
                _ => None,
            },
        )
        .expect("function returns")
}

fn value(entity: EntityId) -> super::model::ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value");
    };
    value
}
