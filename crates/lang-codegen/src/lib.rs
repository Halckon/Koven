//! Koven SSA 与本机代码生成的公共 crate 边界。

pub(crate) mod native;
#[cfg(test)]
mod native_borrow_last_use_cases {
//! Normal source inventories for restricted last-use; no fault/IR generation.
pub(crate) struct Case {
    pub(crate) name: &'static str,
    pub(crate) source: &'static str,
    pub(crate) stdout: &'static [u8],
    pub(crate) allocations: usize,
}

pub(crate) fn cases() -> [Case; 5] {
    [
        Case {
            name: "last-use-chain",
            source: r#"fun <T> view(source: T): borrow T from source = source
fun <T> wrap(source: T): borrow T from source = view(source)
fun consume(own source: String) { println(source) }
fun entry() {
 val source = "kept".clone()
 borrow val parent = view(source)
 borrow val child = wrap(parent)
 borrow val leaf = view(child)
 println(leaf)
 consume(source)
}"#,
            stdout: b"kept\nkept\n",
            allocations: 1,
        },
        Case {
            name: "last-use-projections",
            source: r#"value class Packet(val text: String)
class Resource(val text: String) { deinit() { println("drop") } }
fun <T> view(source: T): borrow T from source = source
fun packetView(source: Packet): borrow String from source = source.text
fun resourceView(source: Resource): borrow String from source = source.text
fun consumePacket(own source: Packet) { println("packet-owned") }
fun consumeResource(own source: Resource) { println("resource-owned") }
fun entry() {
 val packet = Packet("inline".clone())
 borrow val parent = view(packet)
 borrow val text = packetView(parent)
 println(text)
 consumePacket(packet)
 val resource = Resource("heap".clone())
 borrow val resourceParent = view(resource)
 borrow val resourceText = resourceView(resourceParent)
 println(resourceText)
 consumeResource(resource)
}"#,
            stdout: b"inline\npacket-owned\nheap\nresource-owned\ndrop\n",
            allocations: 3,
        },
        Case {
            name: "last-use-resource-lexical",
            source: r#"class Resource(val n: Int) { deinit() { println("drop") } }
fun view(source: Resource): borrow Resource from source = source
fun observe(item: Resource) { println("seen") }
fun entry() {
 val source = Resource(7)
 borrow val item = view(source)
 observe(item)
 println("after")
}"#,
            stdout: b"seen\nafter\ndrop\n",
            allocations: 1,
        },
        Case {
            name: "last-use-map-resource",
            source: r#"class Resource(val n: Int) { deinit() { if (n == 1) { println("drop-one") } else { println("drop-two") } } }
fun observeFirst(item: Resource) { if (item.n == 1) { println("first") } else { error("wrong first") } }
fun observeSecond(item: Resource) { if (item.n == 2) { println("second") } else { error("wrong second") } }
fun entry() {
 var source = mutableMapOf<String, Resource>()
 source.put("key".clone(), Resource(1))
 borrow val first = source.requireValue("key")
 observeFirst(first)
 source.put("key".clone(), Resource(2))
 borrow val second = source.requireValue("key")
 observeSecond(second)
}"#,
            stdout: b"first\ndrop-one\nsecond\ndrop-two\n",
            allocations: 5,
        },
        Case {
            name: "last-use-nullable-slot",
            source: r#"class Token(val n: Int)
fun observe(item: Token?) { if (item == null) { println("null") } else { println("full") } }
fun entry() {
 var source = mutableMapOf<String, Token?>()
 source.put("key".clone(), null)
 borrow val empty = source.requireValue("key")
 observe(empty)
 source.put("key".clone(), Token(7))
 borrow val full = source.requireValue("key")
 observe(full)
 source.put("after".clone(), null)
}"#,
            stdout: b"null\nfull\n",
            allocations: 5,
        },
    ]
}

pub(crate) fn alias_cases() -> [Case; 5] {
    [
        Case {
            name: "alias-multiple-and-child",
            source: r#"fun <T> view(source: T): borrow T from source = source
fun consume(own source: String) { println(source) }
fun entry() {
 val source = "kept".clone()
 borrow val parent = view(source)
 borrow val alias = parent
 borrow val second = (alias)
 borrow val child = view(alias)
 println(child)
 println(parent)
 println(second)
 consume(source)
}"#,
            stdout: b"kept\nkept\nkept\nkept\n",
            allocations: 1,
        },
        Case {
            name: "alias-real-projection-storage",
            source: r#"value class Packet(val text: String)
class Resource(val text: String) { deinit() { println("drop") } }
fun packetView(source: Packet): borrow String from source = source.text
fun resourceView(source: Resource): borrow String from source = source.text
fun consumePacket(own source: Packet) { println("packet-owned") }
fun consumeResource(own source: Resource) { println("resource-owned") }
fun entry() {
 val packet = Packet("inline".clone())
 borrow val parent = packetView(packet)
 borrow val alias = parent
 println(alias)
 consumePacket(packet)
 val resource = Resource("heap".clone())
 borrow val resourceParent = resourceView(resource)
 borrow val resourceAlias = resourceParent
 println(resourceAlias)
 consumeResource(resource)
}"#,
            stdout: b"inline\npacket-owned\nheap\nresource-owned\ndrop\n",
            allocations: 3,
        },
        Case {
            name: "alias-resource-lexical",
            source: r#"class Resource(val n: Int) { deinit() { println("drop") } }
fun view(source: Resource): borrow Resource from source = source
fun observe(item: Resource) { println("seen") }
fun entry() {
 val source = Resource(7)
 borrow val parent = view(source)
 borrow val alias = parent
 observe(alias)
 println("after")
}"#,
            stdout: b"seen\nafter\ndrop\n",
            allocations: 1,
        },
        Case {
            name: "alias-map-resource",
            source: r#"class Resource(val n: Int) { deinit() { if (n == 1) { println("drop-one") } else { println("drop-two") } } }
fun observe(item: Resource) { if (item.n == 1) { println("first") } else { println("second") } }
fun entry() {
 var source = mutableMapOf<String, Resource>()
 source.put("key".clone(), Resource(1))
 borrow val parent = source.requireValue("key")
 borrow val alias = parent
 borrow val sibling = parent
 observe(alias)
 observe(sibling)
 source.put("key".clone(), Resource(2))
 borrow val next = source.requireValue("key")
 borrow val nextAlias = next
 observe(nextAlias)
}"#,
            stdout: b"first\nfirst\ndrop-one\nsecond\ndrop-two\n",
            allocations: 5,
        },
        Case {
            name: "alias-nullable-map-storage",
            source: r#"class Token(val n: Int)
fun observe(item: Token?) { if (item == null) { println("null") } else { println("full") } }
fun entry() {
 var source = mutableMapOf<String, Token?>()
 source.put("key".clone(), null)
 borrow val empty = source.requireValue("key")
 borrow val emptyAlias = empty
 observe(emptyAlias)
 source.put("key".clone(), Token(7))
 borrow val full = source.requireValue("key")
 borrow val fullAlias = full
 observe(fullAlias)
 source.put("after".clone(), null)
}"#,
            stdout: b"null\nfull\n",
            allocations: 5,
        },
    ]
}
}
#[cfg(test)]
mod native_borrow_place_cases {
//! Approved stable initializer sources; normal compilation and exact cleanup only.
use crate::native_borrow_last_use_cases::Case;

pub(crate) fn cases() -> [Case; 5] {
    [
        Case {
            name: "stable-owned-root",
            source: r#"fun view(source: String): borrow String from source = source
fun consume(own source: String) { println(source) }
fun entry() {
 val source = "root".clone()
 borrow val parent = (source)
 borrow val alias = parent
 borrow val child = view(alias)
 println(child)
 println(parent)
 consume(source)
}"#,
            stdout: b"root\nroot\nroot\n",
            allocations: 1,
        },
        Case {
            name: "stable-inline-fields",
            source: r#"value class Packet(val text: String)
value class Envelope(val packet: Packet)
fun consume(own source: Envelope) { println("owned") }
fun entry() {
 val source = Envelope(Packet("inline".clone()))
 borrow val text = (source.packet.text)
 borrow val alias = text
 println(alias)
 consume(source)
}"#,
            stdout: b"inline\nowned\n",
            allocations: 1,
        },
        Case {
            name: "stable-heap-fields",
            source: r#"value class Packet(val text: String)
class Resource(val packet: Packet) { deinit() { println("drop") } }
fun consume(own source: Resource) { println("owned") }
fun entry() {
 val source = Resource(Packet("heap".clone()))
 borrow val text = source.packet.text
 borrow val alias = (text)
 println(alias)
 consume(source)
}"#,
            stdout: b"heap\nowned\ndrop\n",
            allocations: 2,
        },
        Case {
            name: "stable-parent-field",
            source: r#"value class Packet(val text: String)
fun view(source: Packet): borrow Packet from source = source
fun observe(source: Packet) { println("parent") }
fun consume(own source: Packet) { println("owned") }
fun entry() {
 val source = Packet("projected".clone())
 borrow val parent = view(source)
 borrow val text = parent.text
 borrow val alias = text
 println(alias)
 observe(parent)
 consume(source)
}"#,
            stdout: b"projected\nparent\nowned\n",
            allocations: 1,
        },
        Case {
            name: "stable-map-slot-field",
            source: r#"value class Packet(val text: String)
fun entry() {
 var source = mutableMapOf<String, Packet>()
 source.put("key", Packet("slot".clone()))
 borrow val parent = source.requireValue("key")
 borrow val text = parent.text
 borrow val alias = text
 println(alias)
 source.put("key", Packet("changed".clone()))
 println("restored")
}"#,
            stdout: b"slot\nrestored\n",
            // One table buffer and two explicit String clones; the header is an SSA aggregate.
            allocations: 3,
        },
    ]
}
}
#[cfg(test)]
mod native_borrow_storage_cases {
//! single/unit 共用普通源码，覆盖借用 storage 与投影的真实执行。
pub(crate) fn cases(callable: &str) -> Vec<(&'static str, String, &'static [u8])> {
    vec![
        (
            "generic-storage",
            format!(
                r#"
value class Packet(val text: String)
class Resource(val text: String) {{ deinit() {{ println("drop") }} }}
fun <T> view(source: T): borrow T from source = source
fun <T> wrap(source: T): borrow T from source = view(source)
fun observeText(source: String) {{ println(source) }}
fun observePacket(source: Packet) {{ println("packet") }}
fun observeResource(source: Resource) {{ println("resource") }}
fun consumeText(own source: String) {{ println(source) }}
fun consumePacket(own source: Packet) {{ println("packet-owned") }}
fun consumeResource(own source: Resource) {{ println("resource-owned") }}
fun entry() {{
 val text = "kept".clone()
 {{ borrow val item = {callable}(text); observeText(item) }}
 consumeText(text)
 val packet = Packet("payload".clone())
 {{ borrow val item = {callable}(packet); observePacket(item) }}
 consumePacket(packet)
 val resource = Resource("payload".clone())
 {{ borrow val item = {callable}(resource); observeResource(item) }}
 consumeResource(resource)
}}
"#
            ),
            b"kept\nkept\npacket\npacket-owned\nresource\nresource-owned\ndrop\n",
        ),
        (
            "projected-storage",
            format!(
                r#"
value class Packet(val text: String)
class Resource(val text: String) {{ deinit() {{ println("drop") }} }}
fun packetView(source: Packet): borrow String from source = source.text
fun packetWrap(source: Packet): borrow String from source = packetView(source)
fun resourceView(source: Resource): borrow String from source = source.text
fun resourceWrap(source: Resource): borrow String from source = resourceView(source)
fun consumePacket(own source: Packet) {{ println("packet-owned") }}
fun consumeResource(own source: Resource) {{ println("resource-owned") }}
fun entry() {{
 val packet = Packet("inline".clone())
 {{ borrow val item = packet{}(packet); println(item) }}
 consumePacket(packet)
 val resource = Resource("heap".clone())
 {{ borrow val item = resource{}(resource); println(item) }}
 consumeResource(resource)
}}
"#,
                if callable == "view" { "View" } else { "Wrap" },
                if callable == "view" { "View" } else { "Wrap" }
            ),
            b"inline\npacket-owned\nheap\nresource-owned\ndrop\n",
        ),
        (
            "nullable-storage",
            format!(
                r#"
class Token(val n: Int)
fun <T> view(source: T): borrow T from source = source
fun <T> wrap(source: T): borrow T from source = view(source)
fun observe(source: Token?) {{ println("nullable-location") }}
fun consume(own source: Token?) {{
 println("nullable-owned")
}}
fun entry() {{
 val empty: Token? = null
 {{ borrow val item = {callable}(empty); observe(item) }}
 consume(empty)
 val full: Token? = Token(7)
 {{ borrow val item = {callable}(full); observe(item) }}
 consume(full)
}}
"#
            ),
            b"nullable-location\nnullable-owned\nnullable-location\nnullable-owned\n",
        ),
    ]
}
}
#[cfg(test)]
mod native_generated_owner_tests;
#[cfg(test)]
mod native_map_promotion_cases {
//! Ordinary constructor/local operands; no annotated nullable substitute or fault generation.
use crate::native_borrow_last_use_cases::Case;

pub(crate) fn cases() -> [Case; 4] {
    [
        Case {
            name: "nullable-map-resource-delivery",
            source: r#"class Resource(val n: Int) { deinit() { if (n == 1) { println("drop-one") } else { println("drop-two") } } }
fun entry() {
 var source = mutableMapOf<String, Resource?>()
 source.put("key".clone(), Resource(1))
 val replacement = Resource(2)
 source.put("key".clone(), (replacement))
 source.put("key".clone(), null)
}"#,
            stdout: b"drop-one\ndrop-two\n",
            allocations: 6,
        },
        Case {
            name: "nullable-map-move-only-payload",
            source: r#"value class Packet(val text: String)
class Envelope(val packet: Packet)
fun entry() {
 var source = mutableMapOf<String, Envelope?>()
 source.put("key".clone(), Envelope(Packet("one".clone())))
 val replacement = Envelope(Packet("two".clone()))
 source.put("key".clone(), replacement)
 source.put("key".clone(), null)
}"#,
            stdout: b"",
            allocations: 8,
        },
        Case {
            name: "nullable-map-box-move-only-value",
            source: r#"value class Packet(val text: String)
fun entry() {
 var source = mutableMapOf<String, Box<Packet>?>()
 source.put("key".clone(), Box(Packet("one".clone())))
 val replacement = Box(Packet("two".clone()))
 source.put("key".clone(), (replacement))
 source.put("key".clone(), null)
}"#,
            stdout: b"",
            allocations: 8,
        },
        Case {
            name: "nullable-map-string-field-value",
            source: r#"class Label(val text: String)
fun entry() {
 var source = mutableMapOf<String, Label?>()
 source.put("key".clone(), Label("one".clone()))
 val replacement = Label("two".clone())
 source.put("key".clone(), replacement)
 source.put("key".clone(), null)
}"#,
            stdout: b"",
            allocations: 8,
        },
    ]
}
}
#[cfg(test)]
mod native_map_provider_tests;
#[cfg(test)]
mod native_rc_nullable_while_tests {
//! SPEC-0332: bounded presence results through both ordinary native facades.
//! The same four sources and golden outputs are shared with the verified LLVM gate.

use std::process::{Command, Output};

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use crate::{
    emit_native_unit_object,
    native_tests::{TestDirectory, emit_link_and_run},
    ssa::rc_nullable_while_bounded_tests::presence_cases,
};

#[test]
fn single_rc_nullable_while_presence_sources_exit_normally() {
    for (case, source, expected) in presence_cases() {
        let run = emit_link_and_run(&format!("{case}.ko"), source, "entry");
        assert_result(case, "single", &run, expected);
    }
}

#[test]
fn unit_rc_nullable_while_presence_sources_exit_normally() {
    for (case, source, expected) in presence_cases() {
        let run = run_unit(case, source);
        assert_result(case, "unit", &run, expected);
    }
}

fn assert_result(case: &str, entry: &str, run: &Output, expected: &str) {
    assert_eq!(run.status.code(), Some(0), "{case} {entry}: {run:?}");
    assert_eq!(run.stdout, expected.as_bytes(), "{case} {entry}: {run:?}");
    assert!(run.stderr.is_empty(), "{case} {entry}: {run:?}");
    println!("SPEC0332 NATIVE case={case} entry={entry} exit=0 stdout=golden stderr=empty");
}

fn run_unit(case: &str, source_text: &str) -> Output {
    // Match the preflight exactly: one real p source and its own p.entry.
    let path = format!("p/{case}.ko");
    let text = format!("package p\n{source_text}");
    let mut sources = SourceMap::new();
    let source = sources.add_source(&path, &text).expect("unique source");
    let lexed = lex(&sources, source).expect("lexing succeeds");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(&sources, &lexed).expect("parsing succeeds");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let inputs = [SourceUnitInput::new("root", &path, source, &file)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("name resolution succeeds")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .expect("type checking succeeds");
    complete_compilation_unit_types(&sources, &inputs, &names, &environment, &typed)
        .expect("complete types");
    let typed = typed.validate().expect("valid types");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("ownership checking succeeds");
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let owned = owned.validate().expect("valid ownership");
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry"
                && declaration.kind() == SymbolKind::Function
                && names.names().index().packages()[declaration.package().index()]
                    .name()
                    .expect("named package")
                    .segments()
                    .iter()
                    .map(String::as_str)
                    .eq(["p"])
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        panic!("expected exactly one p.entry in {path}");
    };
    assert_eq!(entry.name_span().source_id(), source);
    let directory = TestDirectory::create();
    let object = directory.join("program.o");
    let executable = directory.join("program");
    emit_native_unit_object(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry.id(),
        &object,
    )
    .expect("presence source must emit a native unit object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{case}: {linked:?}");
    Command::new(&executable)
        .output()
        .expect("linked presence executable must launch")
}
}
#[cfg(test)]
mod native_sanitizer_tests;
#[cfg(test)]
pub(crate) mod native_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
#[allow(unused_imports)] pub(crate) use native_borrow_value_field_cases::unit_container_origin_bridge_tests;
#[cfg(test)]
mod unit_type_origin_bridge_tests {
//! Exercise diagnostic mapping with origins produced by real Unit lowering.
//! The backend layout failure is synthetic; this does not claim source overflow.
use lang_frontend::{diagnostic::DiagnosticDetail, name_resolution::SymbolKind};

use crate::{
    NativeObjectErrorKind,
    llvm::{
        LlvmAdapterError,
        layout::{LayoutFailure, LayoutQuantity, TargetLayoutError},
    },
    native::map_backend_error,
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{SsaTypeId, SsaTypeKind},
};

#[test]
fn real_unit_type_origin_bridges_synthetic_layout_failure_to_l0145() {
    let text = "value class Huge(val item: Int)\nfun bootstrap(): Unit { Huge(1) }";
    let analysis = UnitSource::analyze("unit-layout-origin.ko", text).unwrap();
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("bootstrap", SymbolKind::Function))
        .expect("original source lowers through Unit");
    let module = program.module(entry.module()).unwrap();
    let aggregates = module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| {
            matches!(kind, SsaTypeKind::Aggregate { name, .. } if name.starts_with("value#"))
                .then_some(SsaTypeId {
                    module: entry.module(),
                    index,
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(aggregates.len(), 1, "exact original value-class identity");
    let aggregate = aggregates[0];
    let origin = program
        .type_origin(aggregate)
        .expect("Unit lowering, not this test, must publish the type origin");
    let sources = analysis.sources();
    assert_eq!(sources.slice(origin.declaration).unwrap(), "Huge");
    assert_eq!(origin.declaration.start(), 12);
    assert_eq!(origin.declaration.end(), 16);
    assert_eq!(origin.primary.source_id(), origin.declaration.source_id());
    assert_eq!(
        sources.source_name(origin.primary.source_id()).unwrap(),
        "unit-layout-origin.ko"
    );
    assert!(!sources.slice(origin.primary).unwrap().is_empty());
    assert_ne!(
        origin.primary, origin.declaration,
        "retain distinct use and declaration spans"
    );
    let error = map_backend_error(
        sources,
        &program,
        LlvmAdapterError::InvalidLayout(TargetLayoutError {
            ty: aggregate,
            quantity: LayoutQuantity::Size,
            failure: LayoutFailure::ExceedsTarget {
                value: u128::MAX,
                maximum: u64::MAX.into(),
            },
        }),
    );
    assert_eq!(error.kind(), NativeObjectErrorKind::TargetLayout);
    assert_eq!(error.span(), Some(origin.primary));
    let diagnostic = error.diagnostic().expect("real Unit origin enables L0145");
    assert_eq!(diagnostic.code().to_string(), "L0145");
    assert_eq!(diagnostic.primary_span(), origin.primary);
    let labels = diagnostic
        .details()
        .iter()
        .filter_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some(label.span()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, vec![origin.declaration]);
}
}

pub use native::{
    NativeObjectError, NativeObjectErrorKind, NativeUnitEntry, emit_native_const_owned_unit_object,
    emit_native_constant_unit_object, emit_native_owned_unit_object, emit_native_unit_object,
};

// SPEC-0034 完成 SSA→LLVM adapter 后移除该暂时的未使用门禁。
#[allow(dead_code)]
mod llvm;

// SPEC-0033 只建立 crate 内部 SSA；SPEC-0034 接入 lowering 后移除该暂时的未使用门禁。
#[allow(dead_code)]
pub(crate) mod ssa;

#[cfg(test)]
#[allow(unused_imports)] pub(crate) use native_borrow_value_field_cases::bitwise_test_support;

#[cfg(test)]
mod native_map_require_cases {
//! 普通源码的 requireValue 真槽位借用与缺失错误路径。
pub(crate) fn cases() -> Vec<(&'static str, &'static str, &'static [u8])> {
    vec![
        (
            "string",
            r#"fun observe(item: String) { println(item) }
fun entry() {
 var m = mutableMapOf<String, String>()
 val key = "key".clone()
 m.put(key.clone(), "value".clone())
 { borrow val item = m.requireValue(key); observe(item); println(key) }
 m.put("other".clone(), "new".clone())
}"#,
            b"value\nkey\n",
        ),
        (
            "inline",
            r#"value class Packet(val text: String)
fun observe(item: Packet) { println(item.text.clone()) }
fun entry() {
 var m = mutableMapOf<String, Packet>()
 m.put("key".clone(), Packet("inline".clone()))
 { borrow val item = m.requireValue("key"); observe(item) }
 m.put("key".clone(), Packet("replacement".clone()))
}"#,
            b"inline\n",
        ),
        (
            "resource",
            r#"class Resource(val n: Int) { deinit() { if (n == 1) { println("drop-one") } else { println("drop-two") } } }
fun observe(item: Resource) { if (item.n == 1) { println("resource") } else { error("wrong source") } }
fun entry() {
 var m = mutableMapOf<String, Resource>()
 m.put("key".clone(), Resource(1))
 { borrow val item = m.requireValue("key"); observe(item) }
 m.put("key".clone(), Resource(2))
}"#,
            b"resource\ndrop-one\ndrop-two\n",
        ),
        (
            "nullable",
            r#"class Token(val n: Int)
fun observe(item: Token?) { println("nullable-slot") }
fun entry() {
 var m = mutableMapOf<String, Token?>()
 val empty: Token? = null
 m.put("empty".clone(), empty)
 val full: Token? = Token(7)
 m.put("full".clone(), full)
 { borrow val item = m.requireValue("empty"); observe(item) }
 { borrow val item = m.requireValue("full"); observe(item) }
 m.put("after".clone(), null)
}"#,
            b"nullable-slot\nnullable-slot\n",
        ),
    ]
}
pub(crate) const MISSING: &str = "fun entry() { val m = mapOf<String, String>(); borrow val item = m.requireValue(\"missing\"); println(item) }";
}
#[cfg(test)]
mod native_map_with_cases {
//! 普通 scoped callback 源码与 native 输出。
pub(crate) fn cases() -> Vec<(&'static str, String, &'static [u8])> {
    let mut cases = Vec::new();
    for (name, text, expected) in crate::native_map_require_cases::cases() {
        if name == "nullable" {
            continue;
        }
        let source = text.replace("{ borrow val item = m.requireValue(key); observe(item); println(key) }", "if (m.withValue(key, { item -> observe(item); println(key); return })) {} else { error(\"missing key\") }")
   .replace("{ borrow val item = m.requireValue(\"key\"); observe(item) }", "if (m.withValue(\"key\".clone(), { item -> observe(item); return })) {} else { error(\"missing key\") }");
        cases.push((name, source, expected));
    }
    cases.push(("missing", "fun entry() { var m = mutableMapOf<String, String>(); if (m.withValue(\"missing\".clone(), { item -> error(\"must not call\") })) { error(\"must be false\") }; m.put(\"after\".clone(), \"valid\".clone()); println(\"missing\") }".into(), b"missing\n"));
    cases.push(("capture", "fun entry() { var m = mutableMapOf<String,String>(); m.put(\"key\".clone(), \"value\".clone()); val label = \"captured\".clone(); val key=\"key\".clone(); if (m.withValue(key.clone(), { item -> println(label); println(item); return })) {println(key)}; m.put(\"after\".clone(), \"valid\".clone()); println(label) }".into(), b"captured\nvalue\nkey\ncaptured\n"));
    cases.push(("nullable", "class Token(val n:Int)\nfun observe(item:Token?) { if (item == null) { println(\"null\") } else { println(\"value\") } }\nfun entry() { var m=mutableMapOf<String,Token?>(); val empty:Token?=null; m.put(\"empty\".clone(),empty); val full:Token?=Token(7); m.put(\"full\".clone(),full); if (m.withValue(\"missing\", {item -> observe(item)})) {error(\"must be false\")}; if (m.withValue(\"empty\", {item -> observe(item)})) {println(\"found-null\")}; if (m.withValue(\"full\", {item -> observe(item)})) {println(\"found-value\")}; m.put(\"after\".clone(),null) }".into(), b"null\nfound-null\nvalue\nfound-value\n"));
    cases.push(("readonly-missing", "fun inspect(m:Map<String,String>) {if(m.withValue(\"missing\",{item -> error(\"must not call\")})) {error(\"must be false\")};println(\"readonly-missing\")}\nfun entry() {val m=mapOf<String,String>();inspect(m)}".into(),b"readonly-missing\n"));
    cases
}
}

#[cfg(test)]
mod native_owned_map_borrow_cases {
//! SPEC-0299 ordinary source shared by structural SSA and unmodified native tests.
pub(crate) fn core(payload: &str, grouped: bool) -> String {
    let (declaration, value, read) = match payload {
        "String" => ("", "\"value\".clone()", "value"),
        "Text" => (
            "value class Text(val n: Int, val text: String)",
            "Text(7, \"value\".clone())",
            "\"borrowed inline\"",
        ),
        "Resource" => (
            "class Resource(val text: String) { deinit() { println(\"drop\") } }",
            "Resource(\"value\".clone())",
            "value.text",
        ),
        _ => panic!("selected payload"),
    };
    let condition = if grouped {
        "(removed != null)"
    } else {
        "removed != null"
    };
    let after = if payload == "Text" {
        "if (taken.n == 7) { println(\"owned inline\") } else { error(\"wrong payload\") }"
    } else {
        "observe(taken)"
    };
    format!(
        "{declaration}\nfun observe(value: {payload}) {{ println({read}) }}\nfun entry() {{\n var values = mutableMapOf<Int, {payload}>()\n values.put(1, {value})\n val removed = values.remove(1)\n if ({condition}) {{ observe(removed); observe((removed)) }}\n val taken = removed!!\n {after}\n println(\"end\")\n}}"
    )
}

pub(crate) const PAIR: &str = r#"
fun pair(own prefix: String, first: String, second: String) { println(prefix); println(first); println(second) }
fun entry() {
 var values = mutableMapOf<Int, String>()
 values.put(1, "value".clone())
 val removed = values.remove(1)
 if ((removed != null)) { pair("prefix".clone(), removed, (removed)) }
 println(removed!!)
}
"#;
pub(crate) const SCOPES: &str = r#"
fun observe(value: String) { println(value) }
fun entry() {
 var values = mutableMapOf<Int, String>()
 values.put(1, "first".clone()); values.put(2, "second".clone())
 val first = values.remove(1)
 val second = values.remove(2)
 if ((first == null)) { println("unexpected") } else {
   observe(first)
   if ((second != null)) { observe(second); observe(first) }
 }
 if (second != null) { observe(second) }
 println(first!!); println(second!!)
}
"#;
pub(crate) const MISSING: &str = r#"
fun observe(value: String) { println(value) }
fun entry() {
 var mutable = mutableMapOf<Int, String>()
 val removed = mutable.remove(1)
 if ((removed != null)) { observe(removed) } else { println("missing") }
 println("end")
}
"#;
pub(crate) const ABORT: &str = r#"
fun observe(value: String) { println(value) }
fun entry() {
 var values = mutableMapOf<Int, String>()
 val removed = values.remove(1)
 if ((removed != null)) { observe(removed) }
 val taken = removed!!
 observe(taken)
}
"#;
pub(crate) const RESOURCE_SCOPE: &str = r#"
class Resource(val text: String) { deinit() { println(this.text) } }
fun observe(value: Resource) { println(value.text) }
fun entry() {
 val outer = Resource("outer drop".clone())
 var values = mutableMapOf<Int, Resource>()
 values.put(1, Resource("removed drop".clone()))
 val removed = values.remove(1)
 if ((removed != null)) { observe(removed); observe((removed)) }
 println("end")
}
"#;
pub(crate) fn last_use(payload: &str) -> String {
    let source = core(payload, true);
    let (before, _) = source.split_once(" val taken = removed!!").unwrap();
    format!("{before} println(\"end\")\n}}")
}
pub(crate) fn inline_field(unit: bool) -> String {
    let source = core("Text", true);
    if unit {
        source.replace("value class Text(val n: Int, val text: String)", "value class Text(val n: Int, val text: String) { fun number(): Int = this.n }")
          .replace("println(\"borrowed inline\")", "if (value.number() == 7) { println(\"borrowed inline\") } else { error(\"wrong borrow\") }")
    } else {
        source.replace(
            "println(\"borrowed inline\")",
            "if (value.n == 7) { println(\"borrowed inline\") } else { error(\"wrong borrow\") }",
        )
    }
}
pub(crate) fn inline_explicit_clone() -> String {
    core("Text", true).replace(
        "println(\"borrowed inline\")",
        "println(value.text.clone())",
    )
}

pub(crate) fn missing(payload: &str, assert_non_null: bool) -> String {
    let source = if assert_non_null {
        core(payload, true)
    } else {
        last_use(payload)
    };
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("values.put("))
        .map(|line| {
            if !assert_non_null && line.contains("observe((removed)) }") {
                format!("{line} else {{ println(\"missing\") }}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn independent_map() -> String {
    core("String", true).replace(
        "val removed = values.remove(1)",
        "val removed = values.remove(1)\n values.put(1, \"replacement\".clone())",
    )
}

pub(crate) const BORROWED_PREFIX: &str = r#"
fun pair(prefix: String, first: String, second: String) { println(prefix); println(first); println(second) }
fun exercise(prefix: String) {
 var values = mutableMapOf<Int, String>()
 values.put(1, "value".clone())
 val removed = values.remove(1)
 if ((removed != null)) { pair(prefix, removed, (removed)) }
 println(removed!!)
}
fun entry() { exercise("prefix") }
"#;
}

#[cfg(test)]
#[allow(unused_imports)] pub(crate) use native_borrow_value_field_cases::native_multi_borrow_cases;

#[cfg(test)]
mod native_mixed_borrow_cases;

#[cfg(test)]
#[allow(unused_imports)] pub(crate) use test_support::native_nested_borrow_cases;

#[cfg(test)]
pub(crate) mod native_borrow_value_field_cases;
