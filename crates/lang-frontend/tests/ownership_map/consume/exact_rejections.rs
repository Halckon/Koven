//! Exact diagnostic and selected operand contracts for former Single matrix rows.
use super::{
    BuiltinType, Case, CompilationUnitTypes, Expected, ExpressionId, IntrinsicTypeConstructor,
    NameResolution, Observation, ParsedFile, SourceMap, SourceUnitId, SymbolKind, UnitExpressionId,
    observe_unit, parser_test_assertions, unit_map, unit_symbol_named,
};
use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    name_resolution::ReferenceTarget,
    type_checking::ParameterMode,
    type_checking::{
        ContainerConstructionKind, ExpressionCategory, IntrinsicCallable, SequentialContainerKind,
    },
};

pub(super) fn stable_field() {
    // Exact source of the former "stable-field" Single/Rooted matrix row.
    let case = Case {
        label: "stable-field",
        text: r#"class Holder(val source: MutableMap<Int, Int>)
fun run(): Unit {
  val holder = Holder(mutableMapOf<Int, Int>())
  val result = holder.source.consume()
}"#,
        key: BuiltinType::Int,
        value: BuiltinType::Int,
        expected: Expected::Rejected("L0132"),
        calls: 1,
    };

    assert_rejection(
        &case,
        "cannot move a non-Copyable component out of its owner",
        (135, "source"),
        Some((17, "source", "non-Copyable component declared here")),
    );
}

pub(super) fn stable_element() {
    let case = Case {
        label: "stable-element",
        text: r#"fun run(): Unit {
  val maps = listOf(mutableMapOf<Int, Int>())
  val result = maps[0].consume()
}"#,
        key: BuiltinType::Int,
        value: BuiltinType::Int,
        expected: Expected::Rejected("L0136"),
        calls: 1,
    };
    assert_rejection(
        &case,
        "cannot move a non-Copyable element out of a sequential container",
        (79, "maps[0]"),
        Some((
            24,
            "maps",
            "container owner remains responsible for every initialized element",
        )),
    );
}

pub(super) fn temporary() {
    let case = Case {
        label: "temporary",
        text: r#"fun run(): Unit { val result = mutableMapOf<Int, Int>().consume() }"#,
        key: BuiltinType::Int,
        value: BuiltinType::Int,
        expected: Expected::Rejected("L0130"),
        calls: 1,
    };
    assert_rejection(
        &case,
        "MutableMap.consume requires an owned local val/var root",
        (31, "mutableMapOf<Int, Int>()"),
        None,
    );
}

pub(super) fn own_parameter() {
    let case = Case {
        label: "own-parameter",
        text: r#"fun run(own source: MutableMap<Int, Int>): Unit {
  val result = source.consume()
  val count = source.size
}"#,
        key: BuiltinType::Int,
        value: BuiltinType::Int,
        expected: Expected::Rejected("L0130"),
        calls: 1,
    };
    assert_rejection(
        &case,
        "MutableMap.consume requires an owned local val/var root",
        (65, "source"),
        None,
    );
}

pub(super) fn explicit_borrow_local() {
    let case = Case {
        label: "explicit-borrow-local",
        text: r#"fun run(input: MutableMap<Int, Int>): Unit {
  borrow val source = input
  val result = source.consume()
}"#,
        key: BuiltinType::Int,
        value: BuiltinType::Int,
        expected: Expected::Rejected("L0133"),
        calls: 1,
    };
    assert_rejection(
        &case,
        "cannot move a non-Copyable value out of a borrowed binding",
        (88, "source"),
        Some((47, "borrow", "non-owning binding established here")),
    );
}

fn assert_rejection(
    case: &Case,
    message: &str,
    primary: (usize, &str),
    secondary: Option<(usize, &str, &str)>,
) {
    let Expected::Rejected(code) = case.expected else {
        panic!("exact checker expects rejection")
    };
    let mut sources = SourceMap::new();
    let source = sources.add_source("consume.ko", case.text).unwrap();
    let file = parser_test_assertions::parse_file_twice(&sources, source, case.label);
    assert_eq!(sources.source_text(source), Ok(case.text));
    let token = |start, text: &str| {
        let span = sources.span(source, start, start + text.len()).unwrap();
        assert_eq!(sources.slice(span), Ok(text));
        span
    };
    let catalog = DiagnosticCodeCatalog::try_new(&[code]).unwrap();
    let mut diagnostic = Diagnostic::new(
        &sources,
        Severity::Error,
        catalog.resolve(code).unwrap(),
        message,
        token(primary.0, primary.1),
    )
    .unwrap();
    if let Some((start, name, message)) = secondary {
        diagnostic
            .add_label(&sources, token(start, name), message)
            .unwrap();
    }
    let observations =
        [false, true].map(|rooted| observe_unit(case, &sources, &file, rooted, Some(&diagnostic)));
    assert!(
        observations.iter().all(Observation::accepted),
        "{observations:#?}"
    );
}

pub(super) fn assert_indexed_consume_selection(
    case: &Case,
    sources: &SourceMap,
    file: &ParsedFile,
    unit: SourceUnitId,
    names: &NameResolution,
    types: &CompilationUnitTypes,
    calls: &[(ExpressionId, ExpressionId)],
) {
    let [(call, operand)] = calls else {
        panic!("one exact consume selection")
    };
    let call = UnitExpressionId::new(unit, *call);
    let operand = UnitExpressionId::new(unit, *operand);
    let expression_span = |expression: UnitExpressionId| {
        assert_eq!(expression.source_unit(), unit);
        file.ast()
            .expressions()
            .get(expression.expression())
            .unwrap()
            .span()
    };
    let text = |expression| sources.slice(expression_span(expression)).unwrap();
    assert_eq!(text(call), "maps[0].consume()");
    assert_eq!(text(operand), "maps[0]");
    let [selected] = types.map_consumes() else {
        panic!("one actual MapConsume descriptor")
    };
    assert_eq!(selected.expression(), call);
    assert_eq!(selected.receiver(), operand);
    assert!(
        selected.source().is_none(),
        "an indexed place is not a standalone name root"
    );
    assert!(unit_map(
        types,
        Some(selected.receiver_type()),
        IntrinsicTypeConstructor::MutableMap,
        case
    ));
    assert!(unit_map(
        types,
        Some(selected.result_type()),
        IntrinsicTypeConstructor::Map,
        case
    ));
    assert_eq!(types.expression_type(call), Some(selected.result_type()));
    assert_eq!(
        types.expression_type(operand),
        Some(selected.receiver_type())
    );
    assert_eq!(
        Some(selected.key_type()),
        types.types().builtin(BuiltinType::Int)
    );
    assert_eq!(
        Some(selected.value_type()),
        types.types().builtin(BuiltinType::Int)
    );
    assert_eq!(
        types.expression_category(operand),
        Some(ExpressionCategory::Place)
    );
    let slot = types
        .element_place(operand)
        .expect("actual sequential element place");
    assert_eq!(slot.expression(), operand);
    assert_eq!(text(slot.receiver()), "maps");
    assert_eq!(text(slot.index()), "0");
    assert_eq!(slot.container(), SequentialContainerKind::List);
    assert_eq!(slot.element_type(), selected.receiver_type());
    assert!(!slot.is_mutable());

    let [list] = types.container_constructions() else {
        panic!("one owning list construction")
    };
    assert_eq!(text(list.expression()), "listOf(mutableMapOf<Int, Int>())");
    assert_eq!(list.kind(), ContainerConstructionKind::ListForm);
    assert_eq!(list.container(), SequentialContainerKind::List);
    assert_eq!(list.element_type(), slot.element_type());
    assert_eq!(list.parameter_modes(), [ParameterMode::Value]);
    let maps = unit_symbol_named(types, names, unit, "maps").expect("declared container owner");
    let symbol = names.symbols().get(maps.symbol().index()).unwrap();
    assert_eq!(symbol.kind(), SymbolKind::Variable);
    assert_eq!(sources.slice(symbol.span()), Ok("maps"));
    assert_eq!(symbol.span().start(), 24);
    assert!(names.references().iter().any(|reference| reference.span()
        == expression_span(slot.receiver())
        && reference.target() == &ReferenceTarget::Symbol(maps.symbol())));
    assert_eq!(types.symbol_type(maps), Some(list.container_type()));
}

pub(super) fn assert_temporary_consume_selection(
    case: &Case,
    sources: &SourceMap,
    file: &ParsedFile,
    unit: SourceUnitId,
    types: &CompilationUnitTypes,
    calls: &[(ExpressionId, ExpressionId)],
) {
    let [(call, operand)] = calls else {
        panic!("one exact consume selection")
    };
    let spell = |expression| {
        sources
            .slice(file.ast().expressions().get(expression).unwrap().span())
            .unwrap()
    };
    assert_eq!(spell(*call), "mutableMapOf<Int, Int>().consume()");
    assert_eq!(spell(*operand), "mutableMapOf<Int, Int>()");
    let call = UnitExpressionId::new(unit, *call);
    let operand = UnitExpressionId::new(unit, *operand);
    let [selected] = types.map_consumes() else {
        panic!("one selected MapConsume")
    };
    let [created] = types.map_constructions() else {
        panic!("one actual MutableMapOf")
    };
    assert_eq!(selected.expression(), call);
    assert_eq!(selected.receiver(), operand);
    assert!(selected.source().is_none());
    assert_eq!(created.expression(), operand);
    assert_eq!(created.callable(), IntrinsicCallable::MutableMapOf);
    assert_eq!(created.map_type(), selected.receiver_type());
    assert_eq!(types.expression_type(operand), Some(created.map_type()));
    assert_eq!(
        types.expression_category(operand),
        Some(ExpressionCategory::Temporary)
    );
    assert!(unit_map(
        types,
        Some(created.map_type()),
        IntrinsicTypeConstructor::MutableMap,
        case
    ));
    assert!(unit_map(
        types,
        Some(selected.result_type()),
        IntrinsicTypeConstructor::Map,
        case
    ));
    assert_eq!(types.expression_type(call), Some(selected.result_type()));
    assert_eq!(created.key_type(), selected.key_type());
    assert_eq!(created.value_type(), selected.value_type());
    assert_eq!(
        Some(selected.key_type()),
        types.types().builtin(BuiltinType::Int)
    );
    assert_eq!(
        Some(selected.value_type()),
        types.types().builtin(BuiltinType::Int)
    );
}

pub(super) fn assert_own_parameter_consume_selection(
    case: &Case,
    sources: &SourceMap,
    file: &ParsedFile,
    unit: SourceUnitId,
    names: &NameResolution,
    types: &CompilationUnitTypes,
    calls: &[(ExpressionId, ExpressionId)],
) {
    let [(call, operand)] = calls else {
        panic!("one exact consume selection")
    };
    let span = |id: UnitExpressionId| {
        assert_eq!(id.source_unit(), unit);
        file.ast()
            .expressions()
            .get(id.expression())
            .unwrap()
            .span()
    };
    let spell = |id| sources.slice(span(id)).unwrap();
    let call = UnitExpressionId::new(unit, *call);
    let operand = UnitExpressionId::new(unit, *operand);
    let [selected] = types.map_consumes() else {
        panic!("one actual MapConsume selection")
    };
    let [read] = types.map_sizes() else {
        panic!("the original trailing size read must resolve")
    };
    assert_eq!(selected.expression(), call);
    assert_eq!(selected.receiver(), operand);
    assert_eq!(spell(call), "source.consume()");
    assert_eq!(spell(read.expression()), "source.size");
    let (root, root_type) = selected.source().expect("named receiver, not a temporary");
    assert_eq!(root.source_unit(), unit);
    assert_eq!(root_type, selected.receiver_type());
    assert!(unit_map(
        types,
        Some(root_type),
        IntrinsicTypeConstructor::MutableMap,
        case
    ));
    assert!(unit_map(
        types,
        Some(selected.result_type()),
        IntrinsicTypeConstructor::Map,
        case
    ));
    assert_eq!(types.expression_type(call), Some(selected.result_type()));
    assert_eq!(read.map_type(), root_type);
    assert_ne!(read.receiver(), operand);
    assert!(span(read.receiver()).start() > span(operand).start());

    let [run] = types.signatures().declarations() else {
        panic!("exact original top-level run function")
    };
    let run = run.callable().unwrap();
    assert_eq!(run.name(), "run");
    let [parameter] = run.parameters() else {
        panic!("one declared parameter")
    };
    assert_eq!(parameter.symbol(), Some(root));
    assert_eq!(parameter.name(), Some("source"));
    assert_eq!(parameter.mode(), ParameterMode::Value);
    assert_eq!(parameter.ty(), root_type);
    let declaration = names.symbols().get(root.symbol().index()).unwrap();
    assert_eq!(declaration.kind(), SymbolKind::ValueParameter);
    assert_eq!(sources.slice(declaration.span()), Ok("source"));
    assert_eq!(declaration.span().start(), 12);
    for id in [operand, read.receiver()] {
        assert_eq!(spell(id), "source");
        assert_eq!(types.expression_type(id), Some(root_type));
        assert_eq!(
            types.expression_category(id),
            Some(ExpressionCategory::Place)
        );
        assert!(names.references().iter().any(|reference| {
            reference.span() == span(id)
                && reference.target() == &ReferenceTarget::Symbol(root.symbol())
        }));
    }
}

pub(super) fn assert_explicit_borrow_local_selection(
    case: &Case,
    sources: &SourceMap,
    file: &ParsedFile,
    unit: SourceUnitId,
    names: &NameResolution,
    types: &CompilationUnitTypes,
    calls: &[(ExpressionId, ExpressionId)],
) {
    use lang_frontend::parser::{Item, NameMarker, VariableKind};

    let [(call, receiver)] = calls else {
        panic!("one exact consume selection")
    };
    let call = UnitExpressionId::new(unit, *call);
    let receiver = UnitExpressionId::new(unit, *receiver);
    let [selected] = types.map_consumes() else {
        panic!("one selected MapConsume")
    };
    assert_eq!(selected.expression(), call);
    assert_eq!(selected.receiver(), receiver);
    let (local, local_type) = selected.source().expect("named local receiver");
    assert_eq!(local.source_unit(), unit);
    let symbol = |id| names.symbols().get(id).unwrap();
    let local_declaration = symbol(local.symbol().index());
    assert_eq!(local_declaration.kind(), SymbolKind::Variable);
    assert_eq!(sources.slice(local_declaration.span()), Ok("source"));
    assert_eq!(local_type, selected.receiver_type());
    assert!(unit_map(
        types,
        Some(local_type),
        IntrinsicTypeConstructor::MutableMap,
        case
    ));
    assert!(unit_map(
        types,
        Some(selected.result_type()),
        IntrinsicTypeConstructor::Map,
        case
    ));
    assert_eq!(types.expression_type(call), Some(selected.result_type()));

    let mut initializers = file
        .ast()
        .items()
        .iter()
        .filter_map(|(_, item)| match item.payload() {
            Item::Variable {
                kind: VariableKind::BorrowVal(borrow),
                name: NameMarker::Present(name),
                initializer,
                ..
            } if *name == local_declaration.span() => Some((*borrow, *initializer)),
            _ => None,
        });
    let (borrow, initializer) = initializers.next().expect("actual borrow val declaration");
    assert!(initializers.next().is_none());
    assert_eq!(sources.slice(borrow), Ok("borrow"));
    assert_eq!(borrow.start(), 47);
    let initializer = UnitExpressionId::new(unit, initializer);

    let [run] = types.signatures().declarations() else {
        panic!("exact one top-level run")
    };
    let [parameter] = run.callable().unwrap().parameters() else {
        panic!("exact one parameter")
    };
    let input = parameter.symbol().expect("original borrowed parameter");
    assert_ne!(input, local);
    assert_eq!(input.source_unit(), unit);
    assert_eq!(parameter.name(), Some("input"));
    assert_eq!(parameter.mode(), ParameterMode::Borrow);
    assert_eq!(parameter.ty(), local_type);
    let input_declaration = symbol(input.symbol().index());
    assert_eq!(input_declaration.kind(), SymbolKind::ValueParameter);
    assert_eq!(sources.slice(input_declaration.span()), Ok("input"));

    for (id, name, target) in [(receiver, "source", local), (initializer, "input", input)] {
        let span = file
            .ast()
            .expressions()
            .get(id.expression())
            .unwrap()
            .span();
        assert_eq!(sources.slice(span), Ok(name));
        assert_eq!(types.expression_type(id), Some(local_type));
        assert_eq!(
            types.expression_category(id),
            Some(ExpressionCategory::Place)
        );
        assert!(names.references().iter().any(|reference| {
            reference.span() == span
                && reference.target() == &ReferenceTarget::Symbol(target.symbol())
        }));
    }
}
