use lang_frontend::{
    lexer::lex,
    name_resolution::{NameResolution, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{TypeEnvironment, TypedFile, check_types, standard_environments},
};

use super::{
    lower_frontend::{LoweringErrorKind, orchestrate::lower_scalar_file},
    model::{Operation, SequentialContainerKind, SsaTypeKind},
    render::render_program,
};

struct Analysis {
    sources: SourceMap,
    parsed: ParsedFile,
    names: NameResolution,
    #[allow(dead_code)]
    types: TypeEnvironment,
    typed: TypedFile,
    owned: OwnershipCheckedFile,
}

fn analyze(text: &str) -> Analysis {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("container-lowering.ko", text)
        .expect("source must be unique");
    let lexed = lex(&sources, source).expect("lexing must succeed internally");
    let parsed = parse_file(&sources, &lexed).expect("parsing must succeed internally");
    let (environment, types) = standard_environments();
    let names =
        resolve_names(&sources, &parsed, &environment).expect("names must resolve internally");
    let typed =
        check_types(&sources, &parsed, &names, &types).expect("types must check internally");
    let owned = check_ownership(&sources, &parsed, &names, &typed)
        .expect("ownership must check internally");
    Analysis {
        sources,
        parsed,
        names,
        types,
        typed,
        owned,
    }
}

#[test]
fn lowers_string_list_forms_in_source_order_and_drops_each_container_once() {
    let analysis = analyze(
        r#"fun exercise(): Unit {
            val array = arrayOf<String>("array-0", "array-1")
            val list = listOf<String>("list-0", "list-1")
            val mutable = mutableListOf<String>("mutable-0")
            val empty = MutableList<String>()
        }"#,
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("list-form String owners must lower into verified container SSA");
    let module = &program.modules[0];
    let string = module
        .types
        .iter()
        .position(|kind| matches!(kind, SsaTypeKind::StringOwner))
        .expect("String identity must be interned");
    let kinds = module
        .types
        .iter()
        .filter_map(|kind| match kind {
            SsaTypeKind::SequentialContainer { kind, element } if element.index() == string => {
                Some(*kind)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            SequentialContainerKind::Array,
            SequentialContainerKind::List,
            SequentialContainerKind::MutableList,
        ]
    );

    let instructions = &module.functions[0].instructions;
    let literal_bytes = instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::StringLiteral { bytes, .. } => Some(bytes.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        literal_bytes,
        [
            b"array-0".as_slice(),
            b"array-1".as_slice(),
            b"list-0".as_slice(),
            b"list-1".as_slice(),
            b"mutable-0".as_slice(),
        ],
        "element evaluation must retain source order"
    );
    let element_counts = instructions
        .iter()
        .filter_map(|instruction| match &instruction.operation {
            Operation::ContainerConstruct { elements, .. } => Some(elements.len()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(element_counts, [2, 2, 1, 0]);
    assert_eq!(
        instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        4,
        "each held container must own and drop its nested String values exactly once"
    );

    let ssa = render_program(&program);
    assert_eq!(ssa.matches("container.construct").count(), 4, "{ssa}");
    assert_eq!(ssa.matches("drop ").count(), 4, "{ssa}");
}

#[test]
fn moves_a_string_value_into_a_returned_list_without_a_second_owner_drop() {
    let analysis = analyze("fun wrap(own input: String): List<String> = listOf(input)");
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("Value delivery must move the String parameter into the returned container");
    let function = &program.modules[0].functions[0];
    let construct = function
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::ContainerConstruct { elements, .. } => Some(elements),
            _ => None,
        })
        .expect("returned list must be constructed");
    assert_eq!(construct.len(), 1);
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        0,
        "the moved parameter is owned by the returned list, not dropped independently"
    );
}

#[test]
fn runtime_length_container_construction_fails_before_emitting_partial_ssa() {
    let analysis = analyze(
        "fun generate(size: Int, initializer: (Int) -> String): Array<String> = \
         Array<String>(size, initializer)",
    );
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let error = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Ok(_) => panic!("runtime initializer callable bridge remains outside this slice"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}
