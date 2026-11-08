//! Private budget and type-DAG contracts; no public mutable typed-product seam.

use super::*;
use crate::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    type_checking::{IntrinsicTypeConstructor, UnitTypeTable, standard_environments},
};

fn with_checker<R>(text: &str, inspect: impl FnOnce(BodyChecker<'_>) -> R) -> R {
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
    let prepared =
        collect_signatures_with_files(&sources, &inputs, names.names(), &type_environment).unwrap();
    inspect(BodyChecker::new(
        &sources,
        prepared.files,
        names.names(),
        &type_environment,
        prepared.signatures,
    ))
}

fn checked_with_limit(text: &str, limit: usize) -> CompilationUnitTypes {
    let typed = with_checker(text, |checker| {
        checker.run_with_generic_limit(limit).unwrap()
    });
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.clone().validate().is_ok());
    typed
}

#[test]
fn rejected_generic_storage_remains_visible_to_iteration_gates() {
    with_checker(
        "interface Pattern<T>\n\
         class Marker<T> : Pattern<Rc<T>>\n\
         fun <T> take(value: T): Int = 0\n\
         fun entry(entries: Map<Int, Int>): Int {\n\
             for (n in 1..1) {}\n\
             for ((key, value) in entries) {}\n\
             return take<Marker<Nothing>>(if (true) {\n\
                 for (n in if (true) { Marker<Nothing>(); 1..1 } else 1..1) {}\n\
                 for ((key, value) in if (true) { Marker<Nothing>(); entries } else entries) {}\n\
                 0\n\
             } else 0)\n\
         }",
        |checker| {
            let typed = checker.run_with_generic_limit(1024).unwrap();
            assert_eq!(typed.diagnostic_sequential_iterations().len(), 1);
            assert_eq!(typed.diagnostic_map_iterations().len(), 1);
            let [diagnostic] = typed.diagnostics() else {
                panic!("{:?}", typed.diagnostics());
            };
            assert_eq!(diagnostic.code().to_string(), "L0125");
        },
    );
}

#[test]
fn unit_generic_body_skips_methods_without_substitutions() {
    let mut source = String::from("class Factory {\n");
    for index in 0..128 {
        source.push_str(&format!("fun ordinary{index}(): Int = {index}\n"));
    }
    source.push_str("fun <T> generic(): Unit {}\n}\nfun entry(): Unit {}");
    with_checker(&source, |checker| {
        let templates = checker.generic_body_templates().unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates.values().next().unwrap().parameters.len(), 1);
    });
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
fn unit_generic_body_stops_growing_exact_delegation_cycles() {
    let source = "interface Root { fun read(): Int }
                  class Host<T>(val next: Host<List<T>>) : Root by next {}
                  fun entry(host: Host<Int>): Int = host.read()";
    for limit in 0..=2 {
        let typed = checked_with_limit(source, limit);
        assert_eq!(
            typed.runtime_field_layouts().len(),
            2,
            "a repeated declaration and forwarder must not consume the generic frontier"
        );
    }
}

#[test]
fn unit_generic_container_storage_checks_shared_type_nodes() {
    // Each Pair reuses its two child identities. Walking by path would expand 2^48 leaves.
    checked_with_limit(
        "class Pair<A, B>\n\
         fun <T> recurse(): Unit { val values: List<T>? = null; recurse<Pair<T, T>>() }\n\
         fun entry(): Unit { recurse<Int>() }",
        48,
    );
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
