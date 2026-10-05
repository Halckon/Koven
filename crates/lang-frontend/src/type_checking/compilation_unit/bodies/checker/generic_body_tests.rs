//! Private budget and type-DAG contracts; no public mutable typed-product seam.

use super::*;
use crate::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    type_checking::{IntrinsicTypeConstructor, UnitTypeTable, standard_environments},
};

fn checked_with_limit(text: &str, limit: usize) -> CompilationUnitTypes {
    let mut sources = SourceMap::new();
    let source = sources.add_source("main.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment).unwrap();
    let typed = BodyChecker::new(&sources, &inputs, &names, &type_environment, signatures)
        .unwrap()
        .run_with_generic_limit(limit)
        .unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.clone().validate().is_ok());
    typed
}

#[test]
fn unit_generic_body_zero_one_two_budgets_keep_prefix_and_only_one_frontier() {
    for limit in 0..=2 {
        let typed = checked_with_limit(
            "fun <T> first(): Unit { val a: Array<T> = arrayOf(); next<List<T>>() }\n\
             fun <U> next(): Unit { val b: MutableList<U> = mutableListOf(); leaf<U>() }\n\
             fun <V> leaf(): Unit { val c: Array<V> = arrayOf() }\n\
             fun entry(): Unit { first<Int>() }",
            limit,
        );
        let types = typed.types();
        let int = types.builtin(BuiltinType::Int).unwrap();
        let find = |constructor, element| {
            types.find(&UnitTypeKind::Intrinsic {
                constructor,
                arguments: vec![element],
            })
        };
        let list = find(IntrinsicTypeConstructor::List, int)
            .expect("the first frontier publishes its direct call's List<Int> argument");
        assert!(find(IntrinsicTypeConstructor::Array, int).is_some());
        assert_eq!(
            find(IntrinsicTypeConstructor::MutableList, list).is_some(),
            limit >= 1,
            "zero budget must not propagate beyond first's direct frontier"
        );
        assert_eq!(
            find(IntrinsicTypeConstructor::Array, list).is_some(),
            limit >= 2,
            "each specialized pop consumes one budget; leaf has no new type arguments"
        );
        assert_eq!(
            typed.calls().len(),
            3,
            "canonical publication retains template calls"
        );
    }
}

#[test]
fn unit_generic_body_deep_shared_dag_visits_unique_nodes_and_preserves_ids() {
    let mut types = UnitTypeTable::new();
    let int = types.builtin(BuiltinType::Int).unwrap();
    let mut root = int;
    const LAYERS: usize = 4096;
    for _ in 0..LAYERS {
        root = types.intern(UnitTypeKind::Function {
            move_only: false,
            parameters: vec![UnitFunctionParameterType::new(ParameterMode::Borrow, root)],
            return_type: root,
        });
    }
    let length = types.len();
    let mut runtime = ConcreteTypes::default();
    assert!(runtime.is_concrete(&types, root).unwrap());
    assert_eq!(
        runtime.visits,
        LAYERS + 1,
        "two edges share the same completed child"
    );
    assert!(runtime.is_concrete(&types, root).unwrap());
    assert_eq!(
        runtime.visits,
        LAYERS + 1,
        "completed graph is reused across layout queries"
    );
    let mut arguments = ConcreteTypes::for_closed_arguments();
    assert!(arguments.is_concrete(&types, root).unwrap());
    assert_eq!(arguments.visits, LAYERS + 1);
    let mut memo = BTreeMap::new();
    assert_eq!(
        substitute(&mut types, root, &BTreeMap::new(), &mut memo).unwrap(),
        root
    );
    assert_eq!(
        memo.len(),
        LAYERS + 1,
        "substitution also completes each unique node once"
    );
    assert_eq!(
        types.len(),
        length,
        "identity substitution only finds existing canonical nodes"
    );
    assert_eq!(types.builtin(BuiltinType::Int), Some(int));
}

#[test]
fn unit_generic_body_substitution_memos_are_scoped_to_actual_environments() {
    let typed = checked_with_limit(
        "fun <T> probe(own input: T): Int { val xs = listOf(input); return xs.size }",
        2,
    );
    let construction = &typed.container_constructions()[0];
    let UnitTypeKind::Intrinsic { arguments, .. } =
        typed.types().get(construction.container_type()).unwrap()
    else {
        panic!("List<T>");
    };
    let UnitTypeKind::TypeParameter(symbol) = typed.types().get(arguments[0]).unwrap() else {
        panic!("actual source T identity");
    };
    let mut types = typed.types().clone();
    for builtin in [BuiltinType::Int, BuiltinType::String] {
        let actual = types.builtin(builtin).unwrap();
        let result = substitute(
            &mut types,
            construction.container_type(),
            &BTreeMap::from([(*symbol, actual)]),
            &mut BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(
            types.get(result),
            Some(&UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments: vec![actual],
            }),
            "Int's memo must not leak into the next String substitution"
        );
    }
    assert_eq!(
        typed.types().get(arguments[0]),
        Some(&UnitTypeKind::TypeParameter(*symbol))
    );
}

#[test]
fn unit_generic_body_closed_refinement_does_not_expand_runtime_field_support() {
    let typed = checked_with_limit(
        "enum class Shape { Circle, Square }\n\
         fun use(own shape: Shape): Unit { if (shape is Shape.Circle) { val kept = shape } }",
        2,
    );
    let refinement = typed
        .expression_types()
        .values()
        .copied()
        .find(|&ty| matches!(typed.types().get(ty), Some(UnitTypeKind::EnumCase { .. })))
        .expect("a real flow-refined source type");
    assert!(
        ConcreteTypes::for_closed_arguments()
            .is_concrete(typed.types(), refinement)
            .unwrap()
    );
    assert!(
        !ConcreteTypes::default()
            .is_concrete(typed.types(), refinement)
            .unwrap()
    );
}
