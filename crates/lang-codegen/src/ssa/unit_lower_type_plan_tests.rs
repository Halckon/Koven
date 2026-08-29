use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{Operation, SsaTypeKind, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn reachable_body_only_boolean_type_is_planned_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun select(): Int = when {\n\
             true -> 1\n\
             else -> 2\n\
         }\n\
         fun preserve(own amount: Long): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Int {\n\
             val done = p.preserve(1)\n\
             return p.select()\n\
         }",
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
    .expect("reachable body-only Boolean type lowers to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("input permutation preserves body-only scalar type planning");
    assert_eq!(render_program(&forward), render_program(&backward));
    assert_eq!(
        forward.modules[0].types,
        vec![
            SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            },
            SsaTypeKind::Integer {
                bits: 64,
                signed: true,
            },
            SsaTypeKind::Boolean,
        ],
        "all callable signature types precede body-only types"
    );
    let select = forward.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with("koven.p.select.d"))
        .expect("select function exists");
    assert_eq!(
        select
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .filter(|terminator| matches!(terminator.kind, TerminatorKind::Conditional { .. }))
            .count(),
        1
    );
}

#[test]
fn unreachable_body_types_do_not_pollute_the_ssa_type_table() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         fun dead(): Boolean = true\n\
         fun entry(): Int = 1",
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
    .expect("dead Boolean body does not affect reachable Int entry");
    assert_eq!(program.modules[0].functions.len(), 1);
    assert_eq!(
        program.modules[0].types,
        vec![SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        }]
    );
}

#[test]
fn reachable_body_only_string_type_preserves_owner_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\nfun entry(): Unit { val text = \"body-only\" }",
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
    .expect("reachable body-only String owner lowers to verified SSA");
    let module = &program.modules[0];
    assert_eq!(module.types, vec![SsaTypeKind::StringOwner]);
    assert_eq!(
        module.functions[0]
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
            .count(),
        1
    );
    assert_eq!(
        module.functions[0]
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1
    );
}
