use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    source::SourceMap,
    type_checking::standard_environments,
};

use super::{
    model::{Operation, ScalarConstant, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn boolean_subject_else_preserves_semantic_edges() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun select(own flag: Boolean): Int = when (flag) {\n\
             true -> 1\n\
             else -> 2\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.select(true)",
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
    .expect("Boolean subject with else lowers to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("source permutation preserves Boolean-else SSA");
    assert_eq!(render_program(&forward), render_program(&backward));

    let select = function_named(&forward, "koven.p.select.d");
    let (when_true, when_false) = select
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .find_map(|terminator| match &terminator.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => Some((when_true.target, when_false.target)),
            _ => None,
        })
        .expect("select has one Boolean conditional");
    assert_eq!(integer_constant_in(select, when_true), 1);
    assert_eq!(integer_constant_in(select, when_false), 2);
}

#[test]
fn exhaustive_multi_condition_entry_lowers_its_body_once() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun subject(): Boolean = false\n\
         fun select(): Int = when (subject()) {\n\
             true, false -> 7\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.select()",
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
    .expect("one entry covering both Boolean values lowers to verified SSA");
    let select = function_named(&program, "koven.p.select.d");
    assert_eq!(
        direct_call_names(&program, select),
        vec!["koven.p.subject.d0"],
        "the exhaustive specialization must still evaluate its subject once"
    );
    assert_eq!(
        select
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::Constant(ScalarConstant::Integer(7))
            ))
            .count(),
        1,
        "the shared entry body must be lowered once"
    );
    assert!(select.blocks.iter().all(|block| !matches!(
        block.terminator.as_ref().map(|terminator| &terminator.kind),
        Some(TerminatorKind::Conditional { .. })
    )));
}

#[test]
fn dynamic_boolean_subject_condition_compares_once() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun subject(): Boolean = true\n\
         fun candidate(): Boolean = false\n\
         fun select(): Int = when (subject()) {\n\
             candidate() -> 1\n\
             else -> 2\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.select()",
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
    .expect("dynamic Boolean subject condition lowers to verified SSA");
    let select = function_named(&program, "koven.p.select.d");
    assert_eq!(
        direct_call_names(&program, select),
        vec!["koven.p.subject.d0", "koven.p.candidate.d1"],
        "the subject and dynamic candidate are each evaluated once in source order"
    );
    assert_eq!(
        select
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Compare { .. }))
            .count(),
        1,
        "the subject candidate is compared once"
    );
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
fn subjectless_call_conditions_short_circuit_on_false_edges() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun first(): Boolean = true\n\
         fun second(): Boolean = false\n\
         fun select(): Int = when {\n\
             first() -> 1\n\
             second() -> 2\n\
             else -> 3\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.select()",
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
    .expect("subjectless call conditions lower to a verified short-circuit chain");
    let select = function_named(&program, "koven.p.select.d");
    assert_eq!(
        direct_call_names(&program, select),
        vec!["koven.p.first.d0", "koven.p.second.d1"],
        "each condition call is emitted once in source order"
    );
    let entry = select.entry_block().expect("select has an entry block");
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &select
        .block(entry)
        .and_then(|block| block.terminator.as_ref())
        .expect("first condition terminates the entry block")
        .kind
    else {
        panic!("first condition must use a conditional terminator");
    };
    assert_eq!(
        direct_call_names_in_block(&program, select, when_false.target),
        vec!["koven.p.second.d1"],
        "only the first false edge evaluates the second condition"
    );
    assert!(direct_call_names_in_block(&program, select, when_true.target).is_empty());
    assert!(matches!(
        select
            .block(when_false.target)
            .and_then(|block| block.terminator.as_ref())
            .map(|terminator| &terminator.kind),
        Some(TerminatorKind::Conditional { .. })
    ));
}

#[test]
fn subjectless_paths_merge_after_each_consumes_the_same_owner() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun sink(own input: String): Unit {}\n\
         fun route(own first: Boolean, own second: Boolean, own text: String): Unit {\n\
             when {\n\
                 first -> { val done = sink(text) }\n\
                 second -> { val done = sink(text) }\n\
                 else -> { val done = sink(text) }\n\
             }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val done = p.route(false, true, \"owner\") }",
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
    .expect("consistent subjectless owner paths lower to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("source permutation preserves subjectless owner SSA");
    assert_eq!(render_program(&forward), render_program(&backward));
    let route = function_named(&forward, "koven.p.route.d");
    assert_eq!(
        route
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .count(),
        3,
        "each source entry has one independently reachable body"
    );
}

#[test]
fn implicit_subjectless_exit_uses_the_synthetic_branch_index() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun sink(own input: String): Unit {}\n\
         fun route(own flag: Boolean, own text: String): Unit {\n\
             when { flag -> { val done = sink(text) } }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Unit { val done = p.route(false, \"owner\") }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    assert!(owned.ownership().drops().iter().any(|fact| {
        matches!(fact.point(), UnitDropPoint::BranchExit { branch: 1, .. })
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
    .expect("implicit subjectless exit consumes its BranchExit facts");
    let route = function_named(&program, "koven.p.route.d");
    assert_eq!(
        route
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1,
        "only the implicit unmatched branch drops the owner locally"
    );
}

fn function_named<'a>(
    program: &'a super::model::Program,
    prefix: &str,
) -> &'a super::model::Function {
    program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.starts_with(prefix))
        .expect("function exists")
}

fn integer_constant_in(function: &super::model::Function, block: super::model::BlockId) -> i128 {
    function
        .block(block)
        .expect("block belongs to function")
        .instructions
        .iter()
        .filter_map(|instruction| function.instruction(*instruction))
        .find_map(|instruction| match instruction.operation {
            Operation::Constant(ScalarConstant::Integer(value)) => Some(value),
            _ => None,
        })
        .expect("branch materializes its integer result")
}

fn direct_call_names<'a>(
    program: &'a super::model::Program,
    function: &super::model::Function,
) -> Vec<&'a str> {
    function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } => program.modules[0]
                .function(callee)
                .map(|callee| callee.name.as_str()),
            _ => None,
        })
        .collect()
}

fn direct_call_names_in_block<'a>(
    program: &'a super::model::Program,
    function: &super::model::Function,
    block: super::model::BlockId,
) -> Vec<&'a str> {
    function
        .block(block)
        .expect("block belongs to function")
        .instructions
        .iter()
        .filter_map(|instruction| function.instruction(*instruction))
        .filter_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } => program.modules[0]
                .function(callee)
                .map(|callee| callee.name.as_str()),
            _ => None,
        })
        .collect()
}
