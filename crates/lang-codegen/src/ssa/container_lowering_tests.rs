//! Ordinary source construction contracts; source text and ABI oracles remain unchanged.
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{
        DeclarationId, SourceUnitInput, SymbolKind, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{ValidatedCompilationUnitOwnership, owned_compilation_unit_view},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        TypeEnvironment, ValidatedCompilationUnitTypes, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

use super::{
    model::{
        EntityId, Function, Operation, Origin, Program, SequentialContainerKind, SsaTypeKind,
        TerminatorKind,
    },
    render::render_program,
    unit_lower::lower_owned_unit_with_entry,
};

struct Analysis {
    sources: SourceMap,
    parsed: ParsedFile,
    names: ValidatedCompilationUnitNames,
    types: TypeEnvironment,
    typed: ValidatedCompilationUnitTypes,
    owned: ValidatedCompilationUnitOwnership,
    entry: DeclarationId,
}

#[path = "container_lowering_tests/runtime_helpers.rs"]
mod runtime_helpers;

#[path = "container_lowering_tests/runtime_matrix.rs"]
mod runtime_matrix;

#[path = "container_lowering_tests/runtime_matrix_support.rs"]
pub(super) mod runtime_matrix_support;

#[test]
fn runtime_generator_single_source_accepts_pointer_and_shared_initializers() {
    let mut failures = Vec::new();
    for container in ["Array", "List"] {
        for (environment, initializer) in [
            ("pointer", "{ index -> index }"),
            ("shared", "{ index -> index + scale }"),
        ] {
            let text = format!(
                "fun entry(): Int {{ val scale = 7\n\
                 val items = {container}<Int>(3, {initializer})\n\
                 return items[2] }}"
            );
            let analysis = analyze(&text, "entry");
            assert!(analysis.parsed.diagnostics().is_empty());
            assert!(analysis.names.names().diagnostics().is_empty());
            assert!(analysis.typed.types().diagnostics().is_empty());
            assert!(analysis.owned.ownership().diagnostics().is_empty());
            if let Err(error) = lower_unit(&analysis) {
                failures.push(format!("{container}/{environment}: {error:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "existing synchronous Borrow constructors must reach verified source SSA: {failures:?}"
    );
}

fn analyze(text: &str, entry_name: &str) -> Analysis {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("container-lowering.ko", text)
        .expect("source must be unique");
    let lexed = lex(&sources, source).expect("lexing must succeed internally");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).expect("parsing must succeed internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid source input");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("names must resolve internally")
        .validate()
        .expect("names must be valid");
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .expect("types must check internally");
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw)
        .expect("all source types must be complete");
    let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
        .expect("ownership must check internally")
        .into_result()
        .expect("types must remain valid");
    let owned = owned.validate().expect("ownership must be valid");
    let mut entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == entry_name
                && declaration.kind() == SymbolKind::Function
                && declaration.name_span().source_id() == source
        });
    let entry = entries
        .next()
        .expect("selected source function exists")
        .id();
    assert!(
        entries.next().is_none(),
        "selected source function is unique"
    );
    Analysis {
        sources,
        parsed,
        names,
        types,
        typed,
        owned,
        entry,
    }
}

fn lower_unit(analysis: &Analysis) -> Result<super::model::Program, super::LoweringError> {
    let inputs = [SourceUnitInput::memory_document(
        analysis.parsed.source_id(),
        &analysis.parsed,
    )];
    let unit = owned_compilation_unit_view(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.types,
        &analysis.typed,
        &analysis.owned,
    )?;
    lower_owned_unit_with_entry(&unit, analysis.entry).map(|(program, _)| program)
}

fn source_function<'a>(analysis: &Analysis, program: &'a Program, name: &str) -> &'a Function {
    let mut declarations =
        analysis
            .names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == name
                    && declaration.kind() == SymbolKind::Function
                    && declaration.name_span().source_id() == analysis.parsed.source_id()
            });
    let declaration = declarations.next().expect("source function declaration");
    assert!(
        declarations.next().is_none(),
        "source function declaration is unique"
    );
    let span = analysis
        .parsed
        .ast()
        .items()
        .get(declaration.root())
        .unwrap()
        .span();
    let mut functions = program.modules[0]
        .functions
        .iter()
        .filter(|function| function.origin == Origin::Source(span));
    let function = functions
        .next()
        .expect("same-source function body is emitted");
    assert!(
        functions.next().is_none(),
        "one concrete body for this source declaration"
    );
    function
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
        "exercise",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(analysis.typed.types().diagnostics().is_empty());
    assert!(analysis.owned.ownership().diagnostics().is_empty());

    let program = lower_unit(&analysis)
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

    let instructions = &source_function(&analysis, &program, "exercise").instructions;
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
    let analysis = analyze(
        "fun wrap(own input: String): List<String> = listOf(input)",
        "wrap",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(analysis.typed.types().diagnostics().is_empty());
    assert!(analysis.owned.ownership().diagnostics().is_empty());

    let program = lower_unit(&analysis)
        .expect("Value delivery must move the String parameter into the returned container");
    let function = source_function(&analysis, &program, "wrap");
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
fn runtime_length_container_helper_borrows_selected_pointer_initializer() {
    let analysis = analyze(
        "fun generate(size: Int, initializer: (Int) -> String): Array<String> = \
         Array<String>(size, initializer)\n\
         fun entry(): Unit { val items = generate(2, { index -> \"value\" })\n\
         println(items[1]) }",
        "entry",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(analysis.typed.types().diagnostics().is_empty());
    assert!(analysis.owned.ownership().diagnostics().is_empty());
    let program = lower_unit(&analysis)
        .expect("runtime construction borrows the incoming pointer without consuming it");
    let function = source_function(&analysis, &program, "generate");
    let initializer = function
        .block(function.entry_block().unwrap())
        .unwrap()
        .parameters[1];
    let generations = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::ContainerGenerateBorrowed { .. }
            )
        })
        .collect::<Vec<_>>();
    let [generation] = generations.as_slice() else {
        panic!("one container generation")
    };
    let Operation::ContainerGenerateBorrowed {
        initializer: carried,
        ..
    } = generation.operation
    else {
        unreachable!("selected container generation")
    };
    let entry = function.block(function.entry_block().unwrap()).unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &entry.terminator.as_ref().unwrap().kind
    else {
        panic!("runtime length guard branches before initialization")
    };
    assert_eq!(generation.block, when_false.target);
    assert!(matches!(
        function
            .block(when_true.target)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind,
        TerminatorKind::Abort
    ));
    assert_eq!(
        function
            .blocks
            .iter()
            .flat_map(|block| block.terminator.as_ref().unwrap().kind.targets())
            .filter(|target| *target == generation.block)
            .count(),
        1,
        "initializer has one real incoming edge, not a mixed-root join"
    );
    let success = function.block(generation.block).unwrap();
    let slots = success
        .parameters
        .iter()
        .enumerate()
        .filter_map(|(slot, parameter)| (*parameter == EntityId::Loan(carried)).then_some(slot))
        .collect::<Vec<_>>();
    let [slot] = slots.as_slice() else {
        panic!("initializer is one exact success-block loan parameter")
    };
    assert_eq!(
        when_false.arguments[*slot], initializer,
        "guard transports the original incoming pointer without substitution"
    );
    assert_eq!(
        function.entity(EntityId::Loan(carried)).unwrap().ty,
        function.entity(initializer).unwrap().ty
    );
    assert!(
        function
            .instructions
            .iter()
            .all(|instruction| !matches!(instruction.operation, Operation::Drop { .. }))
    );
    crate::llvm::render_verified_program(&program).expect("incoming pointer ABI verifies in LLVM");
}

#[test]
fn mutable_list_add_single_file_lowers_to_container_append() {
    let text = "fun run(): Unit {\n\
                var list = mutableListOf(1)\n\
                list.add(2)\n\
                list.add(3)\n\
                }";
    let analysis = analyze(text, "run");
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(analysis.typed.types().diagnostics().is_empty());
    assert!(analysis.owned.ownership().diagnostics().is_empty());
    let program = lower_unit(&analysis).expect("lowering succeeds");
    let function = source_function(&analysis, &program, "run");
    let appends = function
        .instructions
        .iter()
        .filter(|inst| matches!(inst.operation, Operation::ContainerAppend { .. }))
        .count();
    assert_eq!(appends, 2);
    crate::llvm::render_verified_program(&program).expect("program lowers to LLVM without error");
}

#[test]
fn mutable_list_clear_single_file_lowers_to_container_clear() {
    let text = "fun run(): Unit {\n\
                var list = mutableListOf(1, 2)\n\
                list.clear()\n\
                }";
    let analysis = analyze(text, "run");
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(analysis.typed.types().diagnostics().is_empty());
    assert!(analysis.owned.ownership().diagnostics().is_empty());
    let program = lower_unit(&analysis).expect("lowering succeeds");
    let function = source_function(&analysis, &program, "run");
    let clears = function
        .instructions
        .iter()
        .filter(|inst| matches!(inst.operation, Operation::ContainerClear { .. }))
        .count();
    assert_eq!(clears, 1);
    crate::llvm::render_verified_program(&program).expect("program lowers to LLVM without error");
}

#[test]
fn mutable_list_remove_at_single_file_lowers_to_container_remove_at() {
    let text = "fun run(): Int {\n\
                var list = mutableListOf(10, 20)\n\
                return list.removeAt(0)\n\
                }";
    let analysis = analyze(text, "run");
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(analysis.typed.types().diagnostics().is_empty());
    assert!(analysis.owned.ownership().diagnostics().is_empty());
    let program = lower_unit(&analysis).expect("lowering succeeds");
    let function = source_function(&analysis, &program, "run");
    let remove_ats = function
        .instructions
        .iter()
        .filter(|inst| matches!(inst.operation, Operation::ContainerRemoveAt { .. }))
        .count();
    assert_eq!(remove_ats, 1);
    crate::llvm::render_verified_program(&program).expect("program lowers to LLVM without error");
}

#[test]
fn mutable_list_remove_last_single_file_lowers_to_container_remove_last() {
    let text = "fun run(): Int {\n\
                var list = mutableListOf(10, 20)\n\
                return list.removeLast()\n\
                }";
    let analysis = analyze(text, "run");
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(analysis.typed.types().diagnostics().is_empty());
    assert!(analysis.owned.ownership().diagnostics().is_empty());
    let program = lower_unit(&analysis).expect("lowering succeeds");
    let function = source_function(&analysis, &program, "run");
    let remove_lasts = function
        .instructions
        .iter()
        .filter(|inst| matches!(inst.operation, Operation::ContainerRemoveLast { .. }))
        .count();
    assert_eq!(remove_lasts, 1);
    crate::llvm::render_verified_program(&program).expect("program lowers to LLVM without error");
}

#[test]
fn mutable_list_remove_first_single_file_lowers_to_container_remove_first() {
    let text = "fun run(): Int {\n\
                var list = mutableListOf(10, 20)\n\
                return list.removeFirst()\n\
                }";
    let analysis = analyze(text, "run");
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(analysis.typed.types().diagnostics().is_empty());
    assert!(analysis.owned.ownership().diagnostics().is_empty());
    let program = lower_unit(&analysis).expect("lowering succeeds");
    let function = source_function(&analysis, &program, "run");
    let remove_firsts = function
        .instructions
        .iter()
        .filter(|inst| matches!(inst.operation, Operation::ContainerRemoveFirst { .. }))
        .count();
    assert_eq!(remove_firsts, 1);
    crate::llvm::render_verified_program(&program).expect("program lowers to LLVM without error");
}

#[test]
fn mutable_list_insert_at_single_file_lowers_to_container_insert_at() {
    let text = "fun run(): Unit {\n\
                var list = mutableListOf(10, 20)\n\
                list.insertAt(1, 15)\n\
                }";
    let analysis = analyze(text, "run");
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.names().diagnostics().is_empty());
    assert!(analysis.typed.types().diagnostics().is_empty());
    assert!(analysis.owned.ownership().diagnostics().is_empty());
    let program = lower_unit(&analysis).expect("lowering succeeds");
    let function = source_function(&analysis, &program, "run");
    let insert_ats = function
        .instructions
        .iter()
        .filter(|inst| matches!(inst.operation, Operation::ContainerInsertAt { .. }))
        .count();
    assert_eq!(insert_ats, 1);
    crate::llvm::render_verified_program(&program).expect("program lowers to LLVM without error");
}
