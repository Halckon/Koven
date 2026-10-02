//! SPEC-0244 compilation-unit owned-root atomic replace/swap regressions.

use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

use super::{
    LoweringError, LoweringErrorKind,
    model::{Function, Operation, Program},
    render::render_program,
    unit_lower::{constant::lower_constant_unit_with_entry, lower_scalar_unit_with_entry},
    unit_lower_test_support::{analyze, declaration, parsed},
};

fn lower(text: &str, constants: bool) -> Result<Program, LoweringError> {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/entry.ko",
        &format!("package test\n{text}"),
    );
    let inputs = [SourceUnitInput::new("root", "test/entry.ko", source, &file)];
    let (name_environment, environment) = standard_environments();
    if !constants {
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        return lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        )
        .map(|(program, _)| program);
    }
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    lower_constant_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .map(|(program, _)| program)
}

fn entry(program: &Program) -> &Function {
    program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("test.entry."))
        .unwrap()
}

fn count(function: &Function, prefix: &str) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| format!("{:?}", instruction.operation).starts_with(prefix))
        .count()
}

#[test]
fn unit_root_primitive_replace_and_swap_keep_each_string_owner() {
    for constants in [false, true] {
        let program = lower(
            "fun entry(): Unit {\nvar left = \"left\"\nvar right = \"right\"\nval old = replace(&left, \"new\")\nswap(&left, &right)\nprintln(old)\nprintln(left)\nprintln(right)\n}",
            constants,
        ).expect("owned-root primitives lower to verified SSA");
        let function = entry(&program);
        assert_eq!(count(function, "RootReplace"), 1);
        assert_eq!(count(function, "RootSwap"), 1);
        assert_eq!(count(function, "Drop"), 3);
    }
}

#[test]
fn unit_root_primitive_named_new_value_is_delivered_once() {
    let program = lower(
        "fun entry(): String {\nvar target = \"old\"\nval replacement = \"new\"\nval previous = replace(&target, replacement)\nprintln(target)\nreturn previous\n}", false,
    ).expect("Value delivery removes the named replacement owner");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 1);
    assert_eq!(count(function, "Drop"), 1);
}

#[test]
fn unit_root_primitive_conditional_new_value_keeps_root_and_loan_through_cfg() {
    let program = lower(
        "fun entry(own flag: Boolean): String {\nvar target = \"old\"\nval previous = replace(&target, if (flag) { \"yes\" } else { \"no\" })\nprintln(target)\nreturn previous\n}", true,
    ).expect("pending root owner and exclusive loan are rebound through CFG");
    assert_eq!(count(entry(&program), "RootReplace"), 1);
}

#[test]
fn unit_root_primitive_return_prefix_ends_loan_without_committing() {
    let program = lower(
        "fun entry(): String {\nvar target = \"old\"\nval unused = replace(&target, return \"early\")\nreturn unused\n}", true,
    ).expect("early return cancels the pending primitive before cleanup");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 0);
    assert_eq!(count(function, "BorrowEnd"), 1);
    let end = function
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
        .unwrap();
    let drop = function
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .unwrap();
    assert!(end < drop);
}

#[test]
fn unit_root_primitive_cross_file_input_order_is_deterministic() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun make(): String = \"new\"",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/entry.ko",
        "package q\nfun entry(): String {\nvar target = \"old\"\nval old = replace(&target, p.make())\nprintln(target)\nreturn old\n}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/entry.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let (reverse_names, reverse_typed, reverse_owned) =
        analyze(&sources, &reversed, &name_environment, &environment);
    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .unwrap();
    let (reverse, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .unwrap();
    assert_eq!(render_program(&forward), render_program(&reverse));
}

#[test]
fn unit_root_primitive_inout_parameter_remains_unsupported() {
    let error = lower(
        "fun entry(inout target: String): String = replace(&target, \"new\")",
        false,
    )
    .err()
    .expect("Inout parameter is not an owned root");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn unit_root_primitive_existing_storage_families_lower() {
    for (declarations, ty, first, second) in [
        ("", "Int", "11", "22"),
        ("", "Boolean", "true", "false"),
        (
            "class Item(val number: Int)",
            "Item",
            "Item(11)",
            "Item(22)",
        ),
        (
            "value class Item(val text: String)",
            "Item",
            "Item(\"one\")",
            "Item(\"two\")",
        ),
        (
            "value class Item(val number: Int)",
            "Item",
            "Item(11)",
            "Item(22)",
        ),
        (
            "enum class Item { Full(text: String), Empty }",
            "Item",
            "Item.Full(\"one\")",
            "Item.Full(\"two\")",
        ),
        (
            "value class Item(val number: Int)",
            "Box<Item>",
            "Box(Item(11))",
            "Box(Item(22))",
        ),
        ("", "Rc<Int>", "Rc(11)", "Rc(22)"),
        ("", "List<Int>", "listOf(11)", "listOf(22)"),
        ("", "Array<String>", "arrayOf(\"one\")", "arrayOf(\"two\")"),
        ("", "Rc<Int>?", "null", "Rc(22)"),
    ] {
        let source = format!(
            "{declarations}\nfun entry(): {ty} {{\nvar left: {ty} = {first}\nvar right: {ty} = {second}\nval old = replace(&left, {second})\nswap(&left, &right)\nreturn old\n}}"
        );
        let program = lower(&source, false).unwrap_or_else(|error| panic!("{ty}: {error:?}"));
        assert_eq!(count(entry(&program), "RootReplace"), 1, "{ty}");
        assert_eq!(count(entry(&program), "RootSwap"), 1, "{ty}");
    }
}

#[test]
fn unit_root_primitive_copyable_bindings_do_not_alias_storage() {
    let program = lower(
        "fun entry(): Int {\nvar left = 7\nvar right = left\nval replacement = right\nval old = replace(&left, replacement)\nswap(&left, &right)\nreturn old + left + right\n}", false,
    ).expect("independent copyable mutable roots have independent SSA identities");
    assert_eq!(count(entry(&program), "RootReplace"), 1);
    assert_eq!(count(entry(&program), "RootSwap"), 1);
}

#[test]
fn unit_root_primitive_abort_has_no_commit_or_cleanup() {
    let program = lower(
        "fun entry(): String {\nvar target = \"old\"\nreturn replace(&target, error(\"stop\"))\n}",
        true,
    )
    .expect("aborting new-value expression only lowers the evaluated prefix");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 0);
    assert_eq!(count(function, "Drop"), 0);
}

#[test]
fn unit_root_primitive_projection_boundaries_remain_explicit() {
    let source = "fun entry(): String {\nvar items = arrayOf(\"old\")\nreturn replace(&items[0], \"new\")\n}";
    let error = lower(source, false)
        .err()
        .expect("indexed roots remain outside this slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn unit_root_primitive_conditional_control_exits_cancel_only_the_exit_path() {
    for exit in ["return", "break", "continue"] {
        let source = format!(
            "fun entry(own flag: Boolean): Unit {{\nvar target = \"old\"\nvar once = true\nloop {{\nif (!once) {{ break }}\nonce = false\nval previous = replace(&target, if (flag) {{ {exit} }} else {{ \"new\" }})\nprintln(previous)\nbreak\n}}\nprintln(target)\n}}"
        );
        let program = lower(&source, true).unwrap_or_else(|error| panic!("{exit}: {error:?}"));
        let function = entry(&program);
        assert_eq!(count(function, "RootReplace"), 1, "{exit}");
        assert!(count(function, "BorrowEnd") >= 1, "{exit}");
    }
}

#[test]
fn unit_root_primitive_named_source_calls_do_not_gain_intrinsic_identity() {
    let source = "fun replace(own left: Int, own right: Int): Int = left + right\nfun swap(own left: Int, own right: Int): Int = left - right\nfun entry(): Int = replace(7, swap(4, 2))";
    let program = lower(source, false).expect("same-named declarations are ordinary calls");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 0);
    assert_eq!(count(function, "RootSwap"), 0);
    assert_eq!(count(function, "DirectCall"), 2);
}

#[test]
fn unit_root_primitive_nested_replace_commits_each_distinct_root_once() {
    let program = lower(
        "fun entry(): String {\nvar first = \"first\"\nvar second = \"second\"\nval previous = replace(&first, replace(&second, \"third\"))\nprintln(first)\nprintln(second)\nreturn previous\n}", false,
    ).expect("nested primitive frames preserve the outer exclusive loan");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 2);
    assert_eq!(count(function, "Drop"), 2);
}

#[test]
fn unit_root_primitive_unit_values_materialize_storage() {
    let program = lower(
        "fun make(): Unit {}\nfun entry(): Unit {\nvar left = make()\nvar right = make()\nval old = replace(&left, make())\nswap(&left, &right)\nold\nreturn\n}", false,
    ).expect("Unit roots materialize zero-sized values for atomic operations");
    let function = entry(&program);
    assert_eq!(count(function, "RootReplace"), 1);
    assert_eq!(count(function, "RootSwap"), 1);
    assert_eq!(count(function, "Drop"), 0);
}

#[test]
fn unit_root_primitive_copyable_pending_root_survives_inner_loop() {
    let program = lower(
        "fun entry(own flag: Boolean): Int {\nvar target = 7\nval old = replace(&target, if (flag) { loop { break }\n9 } else { 11 })\nreturn old + target\n}", true,
    ).expect("the pending exclusive root remains the same binding after an inner loop");
    assert_eq!(count(entry(&program), "RootReplace"), 1);
}

#[test]
fn unit_root_primitive_unit_pending_root_survives_conditional_loop_exit() {
    let program = lower(
        "fun make(): Unit {}\nfun entry(own flag: Boolean): Unit {\nvar target = make()\nloop {\nval old = replace(&target, if (flag) { break } else { make() })\nbreak\n}\ntarget\n}", true,
    ).expect("Unit root binding representation stays consistent across early and committed exits");
    assert_eq!(count(entry(&program), "RootReplace"), 1);
}
