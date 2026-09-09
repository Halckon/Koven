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
fn unit_non_null_assertion_consumes_cross_file_rc_owner_once() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/assertion.ko",
        "package p\nfun build(): Rc<Int>? = Rc(37)\nfun unwrap(own owner: Rc<Int>?): Rc<Int> = owner!!",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/assertion.ko",
        "package q\nfun entry(): Int { val owner = p.unwrap(p.build())\n return owner.value }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/assertion.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/assertion.ko", consumer_source, &consumer),
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
    .expect("source-qualified assertion facts must lower to verified SSA");
    let unwrap = function(&program.modules[0], "p.unwrap");
    assert_eq!(
        unwrap
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::NullableTake { .. }))
            .count(),
        1,
        "successful extraction transfers the owner once"
    );
    assert!(
        unwrap.blocks.iter().any(|block| block
            .terminator
            .as_ref()
            .is_some_and(|terminator| matches!(terminator.kind, TerminatorKind::Abort))),
        "null extraction terminates without an ordinary error call"
    );
}

#[test]
fn unit_non_null_assertion_pointer_operand_matrix() {
    for (declarations, ty, construction) in [
        ("class Node()", "Node", "Node()"),
        (
            "value class Token(val item: Int)",
            "Box<Token>",
            "Box(Token(37))",
        ),
        ("", "Rc<Int>", "Rc(37)"),
    ] {
        for operand in ["owner", "((owner))", "build()", "((build()))"] {
            let mut sources = SourceMap::new();
            let text = format!(
                "package p\n{declarations}\nfun build(): {ty}? = {construction}\nfun unwrap(own owner: {ty}?): {ty} = {operand}!!"
            );
            let (source, file) = parsed(&mut sources, "p/assertion.ko", &text);
            let inputs = [SourceUnitInput::new(
                "root",
                "p/assertion.ko",
                source,
                &file,
            )];
            let (name_environment, type_environment) = standard_environments();
            let (names, typed, owned) =
                analyze(&sources, &inputs, &name_environment, &type_environment);
            let (program, _) = lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                declaration(&names, "p", "unwrap"),
            )
            .unwrap_or_else(|error| panic!("{ty}: {operand}: {error:?}"));
            crate::llvm::render_verified_program(&program)
                .unwrap_or_else(|error| panic!("LLVM {ty}: {operand}: {error:?}"));
        }
    }
}

#[test]
fn unit_non_null_assertion_preserves_earlier_call_arguments() {
    for first in ["own first: Rc<Int>", "first: Rc<Int>"] {
        let mut sources = SourceMap::new();
        let text = format!(
            "package p\nfun take({first}, own second: Rc<Int>): Int = second.value\nfun inspect(own first: Rc<Int>, own second: Rc<Int>?): Int = take(first, second!!)"
        );
        let (source, file) = parsed(&mut sources, "p/assertion.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "p/assertion.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let (program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "p", "inspect"),
        )
        .unwrap_or_else(|error| panic!("{first}: {error:?}"));
        crate::llvm::render_verified_program(&program)
            .expect("earlier owner or loan crosses assertion CFG");
    }
}

#[test]
fn unit_non_null_assertion_preserves_call_receiver_and_callable() {
    for text in [
        "package p\nclass Reader(val number: Int) { fun read(item: Rc<Int>): Int = number }\nfun inspect(own item: Rc<Int>?): Int = Reader(3).read(item!!)",
        "package p\nfun read(own item: Rc<Int>): Int = item.value\nfun inspect(own item: Rc<Int>?): Int { val action: (Int) -> Int = { number -> number }\nreturn action(read(item!!)) }",
        "package p\nfun take(own first: Rc<Int>, own second: Rc<Int>): Int = second.value\nfun inspect(flag: Boolean, own a: Rc<Int>, own b: Rc<Int>?): Int = if (flag) { take(a, error(\"stop\")) } else { take(a, b!!) }",
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "p/assertion.ko", text);
        let inputs = [SourceUnitInput::new(
            "root",
            "p/assertion.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let (program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "p", "inspect"),
        )
        .unwrap_or_else(|error| panic!("{text}: {error:?}"));
        crate::llvm::render_verified_program(&program)
            .expect("call target survives argument assertion");
    }
}

#[test]
fn unit_non_null_assertion_preserves_earlier_construction_fields() {
    for (ty, expression) in [
        ("Pair", "Pair(a, b!!)"),
        ("List<Rc<Int>>", "listOf(a, b!!)"),
    ] {
        let mut sources = SourceMap::new();
        let text = format!(
            "package p\nclass Pair(val first: Rc<Int>, val second: Rc<Int>)\nfun inspect(own a: Rc<Int>, own b: Rc<Int>?): {ty} = {expression}"
        );
        let (source, file) = parsed(&mut sources, "p/assertion.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "p/assertion.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let (program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "p", "inspect"),
        )
        .unwrap_or_else(|error| panic!("{expression}: {error:?}"));
        crate::llvm::render_verified_program(&program).expect("construction LLVM verifies");
    }
}

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
