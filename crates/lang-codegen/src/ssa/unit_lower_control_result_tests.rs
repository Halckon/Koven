use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{Function, Operation, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_fact_backed_move_only_control_results_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun choose(own flag: Boolean): String {\n\
             val left = \"left-owner\"\n\
             val right = \"right-owner\"\n\
             val selected = if (flag) { left } else { right }\n\
             return selected\n\
         }\n\
         fun route(own flag: Boolean): String = when (flag) {\n\
             true -> \"when-a\" + \"when-b\"\n\
             false -> \"when-c\" + \"when-d\"\n\
         }\n\
         fun nested(own flag: Boolean): String = if (flag) {\n\
             when (flag) {\n\
                 true -> \"nested-a\" + \"nested-b\"\n\
                 false -> \"nested-c\" + \"nested-d\"\n\
             }\n\
         } else { \"outer-a\" + \"outer-b\" }\n\
         fun diverging(own flag: Boolean): String {\n\
             val selected = if (flag) { \"normal-a\" + \"normal-b\" } else { return \"early\" }\n\
             return selected\n\
         }\n\
         fun label(number: Int): String = \"captured\"",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val chosen = p.choose(true)\n\
             val routed = p.route(false)\n\
             val nested = p.nested(true)\n\
             val escaped = p.diverging(false)\n\
             val action: move (borrow Boolean) -> String = move { flag ->\n\
                 when (flag) {\n\
                     true -> \"lambda-a\" + \"lambda-b\"\n\
                     false -> \"lambda-c\" + \"lambda-d\"\n\
                 }\n\
             }\n\
             val invoked = action(true)\n\
             val offset = 2\n\
             val captured: move (borrow Boolean) -> String = move { flag ->\n\
                 if (flag) { p.label(offset) } else { p.label(offset) }\n\
             }\n\
             val capturedResult = captured(false)\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (reversed_names, reversed_typed, reversed_owned) =
        analyze(&sources, &reversed, &name_environment, &type_environment);

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("fact-backed MoveOnly control results lower to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reversed_names,
        &type_environment,
        &reversed_typed,
        &reversed_owned,
        declaration(&reversed_names, "q", "entry"),
    )
    .expect("input permutation preserves control-result SSA");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let choose = function(module, "p.choose");
    assert_eq!(operation_count(choose, is_string_literal), 2);
    assert_eq!(operation_count(choose, is_drop), 2);
    assert_eq!(terminator_count(choose, is_conditional), 1);

    let route = function(module, "p.route");
    assert_eq!(operation_count(route, is_string_concat), 2);
    assert_eq!(operation_count(route, is_drop), 4);

    let nested = function(module, "p.nested");
    assert_eq!(operation_count(nested, is_string_concat), 3);
    assert_eq!(operation_count(nested, is_drop), 6);
    assert_eq!(terminator_count(nested, is_conditional), 2);

    let diverging = function(module, "p.diverging");
    assert_eq!(operation_count(diverging, is_string_concat), 1);
    assert_eq!(operation_count(diverging, is_drop), 2);
    assert_eq!(terminator_count(diverging, is_return), 2);

    let composite_thunk = module
        .functions
        .iter()
        .find(|function| operation_count(function, is_string_concat) == 2)
        .expect("composite control-result lambda thunk exists");
    assert_eq!(operation_count(composite_thunk, is_drop), 4);
    let captured_thunk = module
        .functions
        .iter()
        .find(|function| operation_count(function, is_shared_field_loan) == 1)
        .expect("captured control-result lambda thunk exists");
    assert_eq!(terminator_count(captured_thunk, is_conditional), 1);
    assert_eq!(
        operation_count(function(module, "q.entry"), is_closure_construct),
        1
    );
}

fn function<'a>(module: &'a super::model::Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .unwrap_or_else(|| panic!("reachable function {name} exists"))
}

fn operation_count(function: &Function, predicate: fn(&Operation) -> bool) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| predicate(&instruction.operation))
        .count()
}

fn terminator_count(function: &Function, predicate: fn(&TerminatorKind) -> bool) -> usize {
    function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .filter(|terminator| predicate(&terminator.kind))
        .count()
}

fn is_string_literal(operation: &Operation) -> bool {
    matches!(operation, Operation::StringLiteral { .. })
}

fn is_string_concat(operation: &Operation) -> bool {
    matches!(operation, Operation::StringConcat { .. })
}

fn is_drop(operation: &Operation) -> bool {
    matches!(operation, Operation::Drop { .. })
}

fn is_closure_construct(operation: &Operation) -> bool {
    matches!(operation, Operation::ClosureConstruct { .. })
}

fn is_shared_field_loan(operation: &Operation) -> bool {
    matches!(operation, Operation::SharedFieldLoan { .. })
}

fn is_conditional(terminator: &TerminatorKind) -> bool {
    matches!(terminator, TerminatorKind::Conditional { .. })
}

fn is_return(terminator: &TerminatorKind) -> bool {
    matches!(terminator, TerminatorKind::Return { .. })
}
