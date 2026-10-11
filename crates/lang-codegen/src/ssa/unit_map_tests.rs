//! SPEC-0288: 编译单元与通用 Map 布局/所有权事实的交接。
use super::{
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn unit_map_owned_remove_strings_inline_and_resources_drop_once() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/entry.ko",
        r#"
package p
value class Text(val text: String)
class Resource { deinit() { println("resource drop") } }
fun entry(): Unit {
 var strings = mutableMapOf<String, String>()
 strings.put("key".clone(), "value".clone())
 val removed = strings.remove("key")
 if (removed != null) { println("present") }
 val taken = removed!!
 println(taken)
 strings.remove("key")
 var texts = mutableMapOf<Int, Text>()
 texts.put(1, Text("inline".clone()))
 val inline = texts.remove(1)!!
 texts.remove(1)
 var resources = mutableMapOf<Int, Resource>()
 resources.put(1, Resource())
 val resource = resources.remove(1)!!
 resources.remove(1)
 println("before exit")
}
"#,
    );
    let inputs = [SourceUnitInput::new("root", "p/entry.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, entry_id) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 7);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"present\nvalue\nbefore exit\nresource drop\n");
}

#[test]
fn unit_map_move_only_subscript_assignment_transfers_named_and_temporary_owners() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/entry.ko",
        r#"
package p
class Resource { deinit() { println("drop") } }
fun entry(): Unit {
 var m = mutableMapOf<String, Resource>()
 val key = "key".clone()
 val value = Resource()
 m[key] = value
 m["key".clone()] = Resource()
 println("after assignment")
}


"#,
    );
    let inputs = [SourceUnitInput::new("root", "p/entry.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, entry_id) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"drop\nafter assignment\ndrop\n");
}

#[test]
fn unit_map_move_only_subscript_assignment_drops_string_entries() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/entry.ko",
        r#"
package p
fun entry(): Unit {
 var m = mutableMapOf<String, String>()
 val key = "key".clone()
 val value = "first".clone()
 m[key] = value
 m["key".clone()] = "second".clone()
 val taken = m.remove("key")!!
 println(taken)
}
"#,
    );
    let inputs = [SourceUnitInput::new("root", "p/entry.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, entry_id) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"second\n");
}

#[test]
fn unit_map_put_preserves_copyable_key_and_value_bindings() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/entry.ko",
        r#"
package p
fun entry(): Unit {
 var m = mutableMapOf<Int, Int>()
 val key = 1
 val value = 0
 m.put(key, value)
 if (m[key] == value && key == 1) { println("put reused") }
 m[key] = value
 if (m[key]!! == value) { println("assignment reused") }
}
"#,
    );
    let inputs = [SourceUnitInput::new("root", "p/entry.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, entry_id) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 1);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"put reused\nassignment reused\n");
}

#[test]
fn unit_map_nullable_value_owned_remove_remains_unsupported() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/entry.ko",
        "package p\nclass Token(val n: Int)\nfun entry(inout m: MutableMap<Int, Token?>): Unit {\n val result = m.remove(1)\n}",
    );
    let inputs = [SourceUnitInput::new("root", "p/entry.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let Err(error) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    ) else {
        panic!("nullable owned remove requires a separate Missing contract")
    };
    assert_eq!(error.kind, super::LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

#[test]
fn unit_map_copyable_queries_cross_file_and_keep_missing_distinct() {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = parsed(
        &mut sources,
        "p/query.ko",
        "package p\nfun query(m: MutableMap<String, Int>, key: String): Int? = m.get(key)",
    );
    let (consumer_id, consumer) = parsed(
        &mut sources,
        "q/entry.ko",
        r#"
package q
import p.query
fun entry(): Unit {
 var m = mutableMapOf<String, Int>()
 m.put("key".clone(), 0)
 val key = "key".clone()
 val hit = query(m, key)
 if (hit != null) { if (hit + 1 == 1) { println("narrowed") } }
 if (hit!! == 0 && query(m, "absent") == null) { println("queried") }
 val removed = m.remove(key)
 if (removed == hit && m[key] == null) { println("removed") }
 println(key)
}
"#,
    );
    let inputs = [
        SourceUnitInput::new("root", "p/query.ko", provider_id, &provider),
        SourceUnitInput::new("root", "q/entry.ko", consumer_id, &consumer),
    ];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let entry = declaration(&names, "q", "entry");
    let (program, entry_id) =
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &types, &typed, &owned, entry)
            .unwrap();
    let (reordered, _) = lower_scalar_unit_with_entry(
        &sources,
        &[inputs[1], inputs[0]],
        &names,
        &types,
        &typed,
        &owned,
        entry,
    )
    .unwrap();
    assert_eq!(render_program(&program), render_program(&reordered));
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 3);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"narrowed\nqueried\nremoved\nkey\n");
}

#[test]
fn unit_map_string_keys_and_move_only_values_execute_with_precise_drops() {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = parsed(
        &mut sources,
        "p/resource.ko",
        "package p\nclass Resource { deinit() { println(\"drop\") } }\nfun make(): MutableMap<String, Resource> = mutableMapOf<String, Resource>()\nfun check(m: MutableMap<String, Resource>, key: String): Unit {\n m.contains(key)\n m.contains(key)\n}",
    );
    let (consumer_id, consumer) = parsed(
        &mut sources,
        "q/entry.ko",
        "package q\nimport p.Resource\nimport p.make\nimport p.check\nfun entry(): Unit {\n var m = make()\n m.put(\"key\".clone(), Resource())\n m.put(\"key\".clone(), Resource())\n val key = \"key\".clone()\n check(m, key)\n println(key)\n}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/resource.ko", provider_id, &provider),
        SourceUnitInput::new("root", "q/entry.ko", consumer_id, &consumer),
    ];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let entry = declaration(&names, "q", "entry");
    let (program, entry_id) =
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &types, &typed, &owned, entry)
            .unwrap();
    let (reordered, _) = lower_scalar_unit_with_entry(
        &sources,
        &[inputs[1], inputs[0]],
        &names,
        &types,
        &typed,
        &owned,
        entry,
    )
    .unwrap();
    assert_eq!(render_program(&program), render_program(&reordered));
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry_id).unwrap();
    let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 6);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"drop\nkey\ndrop\n");
}

#[test]
fn unit_map_operands_follow_argument_control_flow() {
    for operation in [
        "m.put(if (true) { 1 } else { 2 }, 3)",
        "m.put(1, if (true) { 3 } else { 4 })",
        "m.get(if (true) { 1 } else { 2 })",
        "m.contains(if (true) { 1 } else { 2 })",
        "m.remove(if (true) { 1 } else { 2 })",
    ] {
        let source = format!(
            "package p\nfun entry(): Unit {{\n var m = mutableMapOf<Int, Int>()\n {operation}\n m.size\n}}"
        );
        assert_map_unit_source_lowers(&source);
    }
    for key in ["key", "\"key\".clone()"] {
        let source = format!(
            "package p\nfun entry(): Unit {{\n var m = mutableMapOf<String, Int>()\n val key = \"key\".clone()\n m.put({key}, if (true) {{ 3 }} else {{ 4 }})\n m.size\n}}"
        );
        assert_map_unit_source_lowers(&source);
    }
}

#[test]
fn unit_map_mutation_collects_drop_glue_before_abort() {
    for (declarations, body) in [
        (
            "class Resource(val n: Int) { deinit() { println(\"drop\") } }",
            "var m = mutableMapOf<Int, Resource>()\n m.put(1, Resource(1))\n error(\"stop\")",
        ),
        (
            "",
            "var m = mutableMapOf<String, Int>()\n val key = \"key\".clone()\n m.remove(key)\n error(key)",
        ),
    ] {
        let source = format!("package p\n{declarations}\nfun entry(): Unit {{\n {body}\n}}");
        assert_map_unit_source_lowers(&source);
    }
}

#[test]
fn unit_map_put_control_exit_mixed_return_key() {
    let source = "package p\nfun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { return } else { 8 }, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_unit_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn unit_map_put_control_exit_mixed_return_value() {
    let source = "package p\nfun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, if (flag) { return } else { 8 })\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_unit_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn unit_map_put_control_exit_mixed_reversed_key() {
    let source = "package p\nfun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { 8 } else { return }, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_unit_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn unit_map_put_control_exit_mixed_reversed_value() {
    let source = "package p\nfun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, if (flag) { 8 } else { return })\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_unit_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn unit_map_put_control_exit_both_return_key() {
    let source = "package p\nfun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { return } else { return }, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_unit_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn unit_map_put_control_exit_both_return_value() {
    let source = "package p\nfun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, if (flag) { return } else { return })\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_unit_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn unit_map_put_control_exit_direct_return_key() {
    let source = "package p\nfun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(return, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_unit_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn unit_map_put_control_exit_direct_return_value() {
    let source = "package p\nfun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, return)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_unit_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn unit_map_put_control_exit_key_then_value() {
    let source = "package p\nfun inspect(own flag: Boolean, own other: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { return } else { 8 }, if (other) { return } else { 8 })\n m.size\n}\nfun entry(): Unit { inspect(false, false); inspect(false, true); inspect(true, false); inspect(true, true) }";
    let program = assert_map_unit_source_lowers(source);
    assert_map_put_count(&program, 1);
}

fn assert_map_put_count(program: &crate::ssa::model::Program, expected: usize) {
    use crate::ssa::model::Operation;
    let function = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name.contains("inspect"))
        .unwrap();
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::MapPut { .. }))
            .count(),
        expected
    );
    if expected == 0 {
        assert!(
            !function
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.operation, Operation::DirectCall { .. })),
            "a fully diverged put must not lower the suffix call"
        );
    }
}

fn assert_map_unit_source_lowers(text: &str) -> crate::ssa::model::Program {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "p/entry.ko", text);
    let inputs = [SourceUnitInput::new("root", "p/entry.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .unwrap_or_else(|error| panic!("{text}: {error:?}"));
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .unwrap_or_else(|error| panic!("{text}: {error:?}"));
    program
}
