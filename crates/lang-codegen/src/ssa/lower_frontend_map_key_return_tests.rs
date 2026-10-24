//! SPEC-0297: original key-return sources through MemoryDocument and rooted Unit.
#[path = "lower_frontend_map_key_return_unit_identity.rs"]
mod unit_identity;

use crate::ssa::{
    model::{Operation, Program},
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

fn source(operation: &str, key: &str) -> String {
    format!(
        "fun suffix(): Unit {{ println(\"suffix\") }}\n\
         fun inspect(own flag: Boolean): Unit {{\n\
          var m = mutableMapOf<Int, Int>()\n\
          m.{operation}({key})\n\
          suffix()\n\
          m.size\n\
         }}\nfun entry(): Unit {{ inspect(false); inspect(true) }}"
    )
}

fn check_operation(program: &Program, operation: &str, count: usize) {
    let function = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.name.contains("inspect"))
        .unwrap();
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(
                (&i.operation, operation),
                (Operation::MapGet { .. }, "get")
                    | (Operation::MapContains { .. }, "contains")
                    | (Operation::MapRemove { .. }, "remove")
            ))
            .count(),
        count
    );
    if count == 0 {
        assert!(
            !function
                .instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::DirectCall { .. })),
            "full key return must not lower the suffix call"
        );
    }
}

fn single(operation: &str, key: &str, count: usize) {
    let source = source(operation, key);
    unit_identity::check(&source, operation, key, count);
}

fn unit(operation: &str, key: &str, count: usize) {
    let text = format!("package p\n{}", source(operation, key));
    let mut sources = SourceMap::new();
    let (source, file) = unit_lower_test_support::parsed(&mut sources, "p/entry.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/entry.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let (call, receiver) = match operation {
        "get" => (
            typed.types().map_gets()[0].expression(),
            typed.types().map_gets()[0].receiver(),
        ),
        "contains" => (
            typed.types().map_contains_calls()[0].expression(),
            typed.types().map_contains_calls()[0].receiver(),
        ),
        "remove" => (
            typed.types().map_removes()[0].expression(),
            typed.types().map_removes()[0].receiver(),
        ),
        _ => unreachable!(),
    };
    let contract = owned
        .ownership()
        .call_receiver_contract(call)
        .expect("real receiver contract");
    assert_eq!(
        contract.source(),
        lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(receiver)
    );
    assert_eq!(
        owned
            .ownership()
            .drops()
            .iter()
            .filter(|drop| matches!(
                drop.point(),
                lang_frontend::ownership_checking::UnitDropPoint::ControlTransfer(_)
            ))
            .count(),
        if key == "if (flag) { return } else { return }" {
            2
        } else {
            1
        },
        "each actual return cleans the named Map owner"
    );
    let (program, entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        unit_lower_test_support::declaration(&names, "p", "entry"),
    )
    .unwrap_or_else(|error| panic!("{text}\n{error:?}"));
    check_operation(&program, operation, count);
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
}

#[test]
fn single_get_mixed_key_return() {
    single("get", "if (flag) { return } else { 1 }", 1);
}

#[test]
fn single_get_reverse_mixed_key_return() {
    single("get", "if (flag) { 1 } else { return }", 1);
}

#[test]
fn single_get_both_key_return() {
    single("get", "if (flag) { return } else { return }", 0);
}

#[test]
fn single_get_direct_key_return() {
    single("get", "return", 0);
}

#[test]
fn single_contains_mixed_key_return() {
    single("contains", "if (flag) { return } else { 1 }", 1);
}

#[test]
fn single_contains_reverse_mixed_key_return() {
    single("contains", "if (flag) { 1 } else { return }", 1);
}

#[test]
fn single_contains_both_key_return() {
    single("contains", "if (flag) { return } else { return }", 0);
}

#[test]
fn single_contains_direct_key_return() {
    single("contains", "return", 0);
}

#[test]
fn single_remove_mixed_key_return() {
    single("remove", "if (flag) { return } else { 1 }", 1);
}

#[test]
fn single_remove_reverse_mixed_key_return() {
    single("remove", "if (flag) { 1 } else { return }", 1);
}

#[test]
fn single_remove_both_key_return() {
    single("remove", "if (flag) { return } else { return }", 0);
}

#[test]
fn single_remove_direct_key_return() {
    single("remove", "return", 0);
}

#[test]
fn unit_get_mixed_key_return() {
    unit("get", "if (flag) { return } else { 1 }", 1);
}

#[test]
fn unit_get_reverse_mixed_key_return() {
    unit("get", "if (flag) { 1 } else { return }", 1);
}

#[test]
fn unit_get_both_key_return() {
    unit("get", "if (flag) { return } else { return }", 0);
}

#[test]
fn unit_get_direct_key_return() {
    unit("get", "return", 0);
}

#[test]
fn unit_contains_mixed_key_return() {
    unit("contains", "if (flag) { return } else { 1 }", 1);
}

#[test]
fn unit_contains_reverse_mixed_key_return() {
    unit("contains", "if (flag) { 1 } else { return }", 1);
}

#[test]
fn unit_contains_both_key_return() {
    unit("contains", "if (flag) { return } else { return }", 0);
}

#[test]
fn unit_contains_direct_key_return() {
    unit("contains", "return", 0);
}

#[test]
fn unit_remove_mixed_key_return() {
    unit("remove", "if (flag) { return } else { 1 }", 1);
}

#[test]
fn unit_remove_reverse_mixed_key_return() {
    unit("remove", "if (flag) { 1 } else { return }", 1);
}

#[test]
fn unit_remove_both_key_return() {
    unit("remove", "if (flag) { return } else { return }", 0);
}

#[test]
fn unit_remove_direct_key_return() {
    unit("remove", "return", 0);
}
