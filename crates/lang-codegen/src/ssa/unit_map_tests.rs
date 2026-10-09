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
