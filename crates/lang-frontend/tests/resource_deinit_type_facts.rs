//! SPEC-0245: trusted deinit identities and recursive resource classification.

use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::lex,
    name_resolution::{
        NameResolution, SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        resolve_names,
    },
    parser::{Item, ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, ParameterMode, TypeKind, TypedFile, check_compilation_unit_types,
        check_types, standard_environments,
    },
};

fn parse(sources: &mut SourceMap, path: &str, text: &str) -> ParsedFile {
    let source = sources.add_source(path, text).expect("source");
    let lexed = lex(sources, source).expect("lex");
    let file = parse_file(sources, &lexed).expect("parse");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    file
}

fn single(text: &str) -> (ParsedFile, NameResolution, TypedFile) {
    let mut sources = SourceMap::new();
    let file = parse(&mut sources, "resource.ko", text);
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &file, &names).expect("names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    (file, names, typed)
}

fn unit_with_diagnostics(files: &[(&str, &str)]) -> (Vec<ParsedFile>, CompilationUnitTypes) {
    let mut sources = SourceMap::new();
    let parsed = files
        .iter()
        .map(|(path, text)| parse(&mut sources, path, text))
        .collect::<Vec<_>>();
    let inputs = parsed
        .iter()
        .zip(files)
        .map(|(file, (path, _))| SourceUnitInput::new("root", path, file.source_id(), file))
        .collect::<Vec<_>>();
    let (names, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
        .expect("names")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).expect("types");
    (parsed, typed)
}

fn unit(files: &[(&str, &str)]) -> (Vec<ParsedFile>, CompilationUnitTypes) {
    let result = unit_with_diagnostics(files);
    assert!(
        result.1.diagnostics().is_empty(),
        "{:?}",
        result.1.diagnostics()
    );
    result
}

#[test]
fn single_deinit_descriptor_preserves_owner_body_and_readonly_receiver() {
    let (file, _, typed) = single(
        "fun inspect(borrow item: Resource): Unit {}\n\
         class Resource(val label: String) { deinit() { println(label)\ninspect(this) } }",
    );
    let nominal = typed
        .nominals()
        .iter()
        .find(|nominal| nominal.has_deinit())
        .unwrap();
    let deinit = nominal.deinit().expect("trusted deinit descriptor");
    assert_eq!(deinit.owner(), nominal.id());
    assert_eq!(deinit.receiver_mode(), ParameterMode::Borrow);
    assert!(matches!(typed.types().get(deinit.receiver_type()),
        Some(TypeKind::Nominal { nominal: owner, .. }) if *owner == nominal.id()));
    assert!(
        matches!(file.ast().items().get(deinit.item()).unwrap().payload(),
        Item::Deinit { body, .. } if *body == deinit.body())
    );
    assert_eq!(typed.is_resource_type(deinit.receiver_type()), Some(true));
}

#[test]
fn unit_deinit_descriptors_qualify_colliding_local_ast_ids() {
    let (files, typed) = unit(&[
        ("alpha.ko", "class Alpha { deinit() {} }"),
        ("beta.ko", "class Beta { deinit() {} }"),
    ]);
    let deinits = typed
        .signatures()
        .declarations()
        .iter()
        .filter_map(|declaration| declaration.nominal())
        .map(|nominal| {
            let deinit = nominal.deinit().expect("trusted deinit descriptor");
            assert_eq!(deinit.owner(), nominal.declaration());
            assert_eq!(deinit.receiver_type(), nominal.ty());
            assert_eq!(deinit.receiver_mode(), ParameterMode::Borrow);
            assert_eq!(typed.is_resource_type(nominal.ty()), Some(true));
            let file = &files[deinit.item().source_unit().index()];
            assert!(
                matches!(file.ast().items().get(deinit.item().item()).unwrap().payload(),
                Item::Deinit { body, .. } if *body == deinit.body().statement())
            );
            assert_eq!(deinit.item().source_unit(), deinit.body().source_unit());
            deinit
        })
        .collect::<Vec<_>>();
    assert_eq!(deinits.len(), 2);
    assert_eq!(deinits[0].item().item(), deinits[1].item().item());
    assert_ne!(deinits[0].item(), deinits[1].item());
    assert_ne!(deinits[0].body(), deinits[1].body());
}

const CLASSIFICATIONS: &str = "class Resource { deinit() {} }\n\
    class Inner(val resource: Resource)\n\
    class Outer(val inner: Inner)\n\
    class Plain(val value: Int)\n\
    class ResourceCycle(val next: ResourceCycle?, val item: Resource)\n\
    class PlainCycle(val next: PlainCycle?)\n\
    enum class Choice { Some(item: Resource), None }\n\
    class Nullable(val item: Resource?)\n\
    class Boxed(val item: Box<Choice>)\n\
    class Generic<T>(val item: T)\n\
    value class Wrapped(val item: Resource)\n\
    class ArrayOwner(val item: Array<Resource>)\n\
    class ListOwner(val item: List<Resource>)\n\
    class MutableListOwner(val item: MutableList<Resource>)\n\
    class DeclaredGeneric<T> { deinit() {} }\n\
    class GenericInstance(val item: Generic<Resource>)\n\
    class DeclaredGenericInstance(val item: DeclaredGeneric<Int>)\n\
    class MutualA(val next: MutualB?)\n\
    class MutualB(val next: MutualA?, val item: Resource)\n\
    class ResourceFirst(val item: Resource, val next: ResourceFirst?)";

const EXPECTED: &[(&str, Option<bool>)] = &[
    ("Resource", Some(true)),
    ("Inner", Some(true)),
    ("Outer", Some(true)),
    ("Plain", Some(false)),
    ("ResourceCycle", Some(true)),
    ("PlainCycle", Some(false)),
    ("Choice", Some(true)),
    ("Nullable", Some(true)),
    ("Boxed", Some(true)),
    ("Generic", None),
    ("Wrapped", Some(true)),
    ("ArrayOwner", Some(true)),
    ("ListOwner", Some(true)),
    ("MutableListOwner", Some(true)),
    ("DeclaredGeneric", Some(true)),
    ("GenericInstance", Some(true)),
    ("DeclaredGenericInstance", Some(true)),
    ("MutualA", Some(true)),
    ("MutualB", Some(true)),
    ("ResourceFirst", Some(true)),
];

#[test]
fn single_resource_classification_follows_fields_payloads_and_cycles() {
    let (_, names, typed) = single(CLASSIFICATIONS);
    for &(name, expected) in EXPECTED {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == name)
            .unwrap()
            .id();
        let ty = typed.symbol_type(symbol).expect("nominal type");
        assert_eq!(typed.is_resource_type(ty), expected, "{name}");
    }
}

#[test]
fn unit_resource_classification_follows_fields_payloads_and_cycles() {
    let (_, typed) = unit(&[("types.ko", CLASSIFICATIONS)]);
    for &(name, expected) in EXPECTED {
        // The unit product is source-ordered just like this declaration matrix.
        let index = EXPECTED.iter().position(|entry| entry.0 == name).unwrap();
        let declaration = &typed.signatures().declarations()[index];
        assert_eq!(typed.is_resource_type(declaration.ty()), expected, "{name}");
    }
}

#[test]
fn unit_deinit_body_can_read_fields_and_share_borrow_this() {
    let (_, typed) = unit(&[(
        "body.ko",
        "fun inspect(borrow item: Resource): Unit {}\n\
         class Resource(val label: String) { deinit() { println(label)\ninspect(this) } }",
    )]);
    let deinit = typed
        .signatures()
        .declarations()
        .iter()
        .filter_map(|declaration| declaration.nominal())
        .find_map(|nominal| nominal.deinit())
        .expect("deinit");
    assert_eq!(deinit.receiver_mode(), ParameterMode::Borrow);
    assert_eq!(typed.calls().len(), 2);
}

const GENERIC_CLASSIFICATIONS: &str = "class Resource { deinit() {} }\n\
    class Generic<T>(val item: T)\n\
    class Nested<T>(val item: Generic<Generic<T>>)\n\
    class Grow<T>(val item: T, val next: Grow<List<T>>?)\n\
    class Swap<A, B>(val item: A, val next: Swap<B, A>?)\n\
    class Phantom<T>\n\
    class IntOwner(val item: Generic<Int>)\n\
    class StringOwner(val item: Generic<String>)\n\
    class ResourceOwner(val item: Generic<Resource>)\n\
    class NestedMemory(val item: Nested<String>)\n\
    class NestedResource(val item: Nested<Resource>)\n\
    class GrowingMemory(val item: Grow<String>)\n\
    class GrowingResource(val item: Grow<Resource>)\n\
    class SwappedMemory(val item: Swap<Int, String>)\n\
    class SwappedResource(val item: Swap<String, Resource>)\n\
    class PhantomResource(val item: Generic<Phantom<Resource>>)";

const GENERIC_EXPECTED: &[(&str, Option<bool>)] = &[
    ("Resource", Some(true)),
    ("Generic", None),
    ("Nested", None),
    ("Grow", None),
    ("Swap", None),
    ("Phantom", Some(false)),
    ("IntOwner", Some(false)),
    ("StringOwner", Some(false)),
    ("ResourceOwner", Some(true)),
    ("NestedMemory", Some(false)),
    ("NestedResource", Some(true)),
    ("GrowingMemory", Some(false)),
    ("GrowingResource", Some(true)),
    ("SwappedMemory", Some(false)),
    ("SwappedResource", Some(true)),
    ("PhantomResource", Some(false)),
];

#[test]
fn single_resource_classification_substitutes_concrete_recursive_generics() {
    let (_, names, typed) = single(GENERIC_CLASSIFICATIONS);
    for &(name, expected) in GENERIC_EXPECTED {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == name)
            .unwrap()
            .id();
        assert_eq!(
            typed.is_resource_type(typed.symbol_type(symbol).unwrap()),
            expected,
            "{name}"
        );
    }
}

#[test]
fn unit_resource_classification_substitutes_concrete_recursive_generics() {
    let (_, typed) = unit(&[("generics.ko", GENERIC_CLASSIFICATIONS)]);
    for (index, &(name, expected)) in GENERIC_EXPECTED.iter().enumerate() {
        let declaration = &typed.signatures().declarations()[index];
        assert_eq!(typed.is_resource_type(declaration.ty()), expected, "{name}");
    }
}

#[test]
fn unit_deinit_accepts_unit_return_and_its_own_loop_jumps() {
    unit(&[(
        "return.ko",
        "class Resource { deinit() { println(\"done\")\nreturn } }",
    )]);
    unit(&[(
        "loop.ko",
        "class Resource { deinit() { while (true) { break }\nreturn } }",
    )]);
}

#[test]
fn unit_deinit_rejects_return_value_without_unrelated_annotation_label() {
    let (_, typed) = unit_with_diagnostics(&[(
        "return.ko",
        "fun previous(): Int = 1\nclass Resource { deinit() { return 1 } }",
    )]);
    let diagnostics = typed.diagnostics();
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d.code().to_string())
            .collect::<Vec<_>>(),
        ["L0087"]
    );
    assert_eq!(
        diagnostics[0]
            .details()
            .iter()
            .filter(|detail| matches!(detail, DiagnosticDetail::Label(_)))
            .count(),
        0,
        "implicit Unit must not inherit previous callable annotation"
    );
}

#[test]
fn unit_deinit_rejects_loop_jumps_without_a_body_loop() {
    for keyword in ["break", "continue"] {
        let text = format!("class Resource {{ deinit() {{ {keyword} }} }}");
        let (_, typed) = unit_with_diagnostics(&[("jump.ko", &text)]);
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0142"]
        );
    }
}

#[test]
fn unit_resource_classification_cache_does_not_change_product_equality() {
    let (_, typed) = unit(&[("equality.ko", "class Resource { deinit() {} }")]);
    let before = typed.clone();
    let ty = typed.signatures().declarations()[0].ty();
    assert_eq!(typed.is_resource_type(ty), Some(true));
    assert_eq!(typed, before);
    assert_eq!(format!("{typed:?}"), format!("{before:?}"));
    let after = typed.clone();
    assert_eq!(before, after);
    assert_eq!(before.is_resource_type(ty), Some(true));
    assert_eq!(before, after);
}
