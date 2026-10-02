use super::*;

#[test]
fn delegated_dispatch_owner_recipe_keeps_unsupported_nested_kinds_closed() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/dispatch-recipes.ko",
        "package p\n\
         value class ValueWrapper<T>(val item: T)\n\
         class Pair<A, B>(val first: A, val second: B)\n\
         class ArrayOwner<T>(val item: Array<T>)\n\
         class NullableOwner<T>(val item: T?)\n\
         class FunctionOwner<T>(val item: (T) -> T)\n\
         class ValueOwner<T>(val item: ValueWrapper<T>)\n\
         class MultiOwner<T>(val item: Pair<T, T>)\n\
         fun entry(\n\
             array: ArrayOwner<Int>,\n\
             nullable: NullableOwner<Int>,\n\
             callable: FunctionOwner<Int>,\n\
             wrapped: ValueOwner<Int>,\n\
             multi: MultiOwner<Int>\n\
         ): Unit {}",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/dispatch-recipes.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");

    for owner_name in [
        "ArrayOwner",
        "NullableOwner",
        "FunctionOwner",
        "ValueOwner",
        "MultiOwner",
    ] {
        let owner = typed
            .types()
            .signatures()
            .declaration(declaration(&names, owner_name))
            .and_then(|signature| signature.nominal())
            .unwrap_or_else(|| panic!("{owner_name} signature"));
        let [parameter] = owner.type_parameters() else {
            panic!("{owner_name} has one type parameter");
        };
        let [field] = owner.fields() else {
            panic!("{owner_name} has one field");
        };
        let substitutions = BTreeMap::from([(*parameter, int)]);
        let error = resolve_delegated_dispatch_owner_argument(
            typed.types(),
            field.ty(),
            &substitutions,
            field.span(),
            &mut BTreeSet::new(),
        )
        .unwrap_err();
        assert_eq!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{owner_name}"
        );
        assert_eq!(error.span, Some(field.span()), "{owner_name}");
    }
}

#[test]
fn inherited_dispatch_owner_recipe_keeps_dependent_kinds_and_missing_canonical_closed() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/inherited-recipes.ko",
        "package p\n\
         value class ValueWrapper<T>(val item: T)\n\
         class Wrapper<T>(val item: T)\n\
         class Pair<A, B>(val first: A, val second: B)\n\
         class ArrayOwner<T>(val item: Array<T>)\n\
         class NullableOwner<T>(val item: T?)\n\
         class FunctionOwner<T>(val item: (T) -> T)\n\
         class ValueOwner<T>(val item: ValueWrapper<T>)\n\
         class MultiOwner<T>(val item: Pair<T, T>)\n\
         class NominalOwner<T>(val item: Wrapper<T>)\n\
         class GrowingOwner<T>(val item: GrowingOwner<List<T>>)\n\
         class SelfNode<T>(val next: SelfNode<T>)\n\
         class SelfOwner<T>(val item: SelfNode<T>)\n\
         class LeftNode<T>(val right: RightNode<T>)\n\
         class RightNode<T>(val left: LeftNode<T>)\n\
         class MutualOwner<T>(val item: LeftNode<T>)\n\
         class ClosedSelf<T>(val item: T, val next: ClosedSelf<Int>)\n\
         class ClosedSelfOwner<T>(val item: ClosedSelf<T>)\n\
         class ClosedLeft<T>(val item: T, val right: ClosedRight<Int>)\n\
         class ClosedRight<T>(val item: T, val left: ClosedLeft<Int>)\n\
         class ClosedMutualOwner<T>(val item: ClosedLeft<T>)\n\
         class ParamLeft<T>(val right: ParamRight<Int>)\n\
         class ParamRight<U>(val left: ParamLeft<U>)\n\
         class ParamCycleOwner<T>(val item: ParamLeft<T>)\n\
         enum class ClosedChoice { Item(item: ClosedEnumNode<Int>), Empty }\n\
         class ClosedEnumNode<T>(val item: T, val choice: ClosedChoice)\n\
         class ClosedEnumOwner<T>(val item: ClosedEnumNode<T>)\n\
         class ClosedLeaf<U>(val items: List<U>)\n\
         class ClosedDag<T>(val item: T, val leaf: ClosedLeaf<Int>)\n\
         class ClosedDagOwner<T>(val item: ClosedDag<T>)\n\
         fun closedDagSeed(input: ClosedDag<Int>, items: List<Int>): Unit {}\n\
         class ListOwner<T>(val item: List<T>)\n\
         class Marker<T>(val marker: Int)\n\
         class MarkerOwner<T>(val item: Marker<T>)\n\
         fun entry(): Long = 1L",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/inherited-recipes.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, _) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let int = typed
        .types()
        .types()
        .builtin(BuiltinType::Int)
        .expect("Int type");

    for owner_name in [
        "ArrayOwner",
        "NullableOwner",
        "FunctionOwner",
        "ValueOwner",
        "MultiOwner",
        "GrowingOwner",
        "SelfOwner",
        "MutualOwner",
        "ClosedSelfOwner",
        "ClosedMutualOwner",
        "ParamCycleOwner",
        "ClosedEnumOwner",
    ] {
        let owner = typed
            .types()
            .signatures()
            .declaration(declaration(&names, owner_name))
            .and_then(|signature| signature.nominal())
            .unwrap_or_else(|| panic!("{owner_name} signature"));
        let [parameter] = owner.type_parameters() else {
            panic!("{owner_name} has one type parameter");
        };
        let [field] = owner.fields() else {
            panic!("{owner_name} has one field");
        };
        let error = resolve_inherited_dispatch_owner_argument(
            typed.types(),
            field.ty(),
            &BTreeMap::from([(*parameter, int)]),
            field.span(),
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
        )
        .unwrap_err();
        assert_eq!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{owner_name}"
        );
        if matches!(
            owner_name,
            "SelfOwner"
                | "MutualOwner"
                | "ClosedSelfOwner"
                | "ClosedMutualOwner"
                | "ParamCycleOwner"
                | "ClosedEnumOwner"
        ) {
            assert!(error.span.is_some(), "{owner_name}");
        } else {
            assert_eq!(error.span, Some(field.span()), "{owner_name}");
        }
    }

    let list_owner = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "ListOwner"))
        .and_then(|signature| signature.nominal())
        .expect("ListOwner signature");
    let [parameter] = list_owner.type_parameters() else {
        panic!("ListOwner has one type parameter");
    };
    let [field] = list_owner.fields() else {
        panic!("ListOwner has one field");
    };
    let long = typed
        .types()
        .types()
        .builtin(BuiltinType::Long)
        .expect("Long type");
    assert!(
        typed
            .types()
            .types()
            .find(&UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments: vec![long],
            })
            .is_none(),
        "fixture must not pre-intern List<Long>"
    );
    let error = resolve_inherited_dispatch_owner_argument(
        typed.types(),
        field.ty(),
        &BTreeMap::from([(*parameter, long)]),
        field.span(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )
    .unwrap_err();
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert_eq!(error.span, Some(field.span()));

    let dag_owner = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "ClosedDagOwner"))
        .and_then(|signature| signature.nominal())
        .expect("ClosedDagOwner signature");
    let [parameter] = dag_owner.type_parameters() else {
        panic!("ClosedDagOwner has one type parameter");
    };
    let [field] = dag_owner.fields() else {
        panic!("ClosedDagOwner has one field");
    };
    resolve_inherited_dispatch_owner_argument(
        typed.types(),
        field.ty(),
        &BTreeMap::from([(*parameter, int)]),
        field.span(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )
    .expect("closed generic List substitution is a finite recipe");

    let marker = declaration(&names, "Marker");
    let marker_owner = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "MarkerOwner"))
        .and_then(|signature| signature.nominal())
        .expect("MarkerOwner signature");
    let [parameter] = marker_owner.type_parameters() else {
        panic!("MarkerOwner has one type parameter");
    };
    let [field] = marker_owner.fields() else {
        panic!("MarkerOwner has one field");
    };
    assert!(
        typed
            .types()
            .types()
            .find(&UnitTypeKind::Nominal {
                declaration: marker,
                arguments: vec![long],
            })
            .is_none(),
        "fixture must not pre-intern Marker<Long>"
    );
    let error = resolve_inherited_dispatch_owner_argument(
        typed.types(),
        field.ty(),
        &BTreeMap::from([(*parameter, long)]),
        field.span(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
    )
    .unwrap_err();
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert_eq!(error.span, Some(field.span()));
}

#[test]
fn remaps_requirement_arguments_to_concrete_owner_and_callable_slots() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface GenericBase<A> {\n\
             fun <R> id(own input: R): R\n\
             fun <R> throughRequirement(own input: R): R = this.id(input)\n\
         }\n\
         class Host<T>: GenericBase<String> {\n\
             override fun <R> id(own input: R): R = input\n\
         }\n\
         fun entry(host: Host<Int>): Long = host.throughRequirement(2L)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let host = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Host"))
        .and_then(|signature| signature.nominal())
        .expect("Host signature");
    let implementation = host
        .members()
        .iter()
        .find(|member| member.name() == "id")
        .expect("generic concrete override")
        .target();

    let instances = plan(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    );
    let implementation = instances
        .iter()
        .find(|instance| instance.key().target() == implementation)
        .expect("concrete generic override instance");
    assert_eq!(implementation.key().type_arguments().len(), 2);
    assert!(matches!(
        typed
            .types()
            .types()
            .get(implementation.key().type_arguments()[0]),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(matches!(
        typed
            .types()
            .types()
            .get(implementation.key().type_arguments()[1]),
        Some(UnitTypeKind::Builtin(BuiltinType::Long))
    ));
}

#[test]
fn remaps_inherited_default_owner_recipe_and_callable_slots() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Base<A> {\n\
             fun <R> read(own input: R): R\n\
             fun <R> throughRequirement(own input: R): R = this.read(input)\n\
         }\n\
         interface Derived<B>: Base<String> {\n\
             fun <R> read(own input: R): R = input\n\
         }\n\
         class Host<X, Y>: Derived<Y> {}\n\
         fun entry(host: Host<Int, Long>): Int = host.throughRequirement(2)",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let derived = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Derived"))
        .and_then(|signature| signature.nominal())
        .expect("Derived signature");
    let implementation = derived
        .members()
        .iter()
        .find(|member| member.name() == "read")
        .expect("inherited default implementation")
        .target();
    let requirement = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Base"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| {
            nominal
                .members()
                .iter()
                .find(|member| member.name() == "read")
        })
        .expect("ancestor abstract requirement")
        .target();

    let instances = plan(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    );
    let implementation = instances
        .iter()
        .find(|instance| instance.key().target() == implementation)
        .expect("inherited generic default instance");
    assert_eq!(implementation.key().type_arguments().len(), 2);
    assert!(matches!(
        typed
            .types()
            .types()
            .get(implementation.key().type_arguments()[0]),
        Some(UnitTypeKind::Builtin(BuiltinType::Long))
    ));
    assert!(matches!(
        typed
            .types()
            .types()
            .get(implementation.key().type_arguments()[1]),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(implementation.key().static_self().is_some());
    assert!(
        instances
            .iter()
            .all(|instance| instance.key().target() != requirement)
    );
}

#[test]
fn remaps_list_inherited_owner_recipe_to_the_effective_default() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/list-inherited.ko",
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Host<Y>: Derived<List<Y>> {}\n\
         fun entry(host: Host<Int>): Int = host.throughRequirement()",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "p/list-inherited.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let derived = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Derived"))
        .and_then(|signature| signature.nominal())
        .expect("Derived signature");
    let implementation = derived
        .members()
        .iter()
        .find(|member| member.name() == "read")
        .expect("inherited default implementation")
        .target();
    let requirement = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "Base"))
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| {
            nominal
                .members()
                .iter()
                .find(|member| member.name() == "read")
        })
        .expect("ancestor abstract requirement")
        .target();

    let instances = plan_unit_instances(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "entry"),
    )
    .expect("List owner recipe must select the exact inherited default");
    let implementation = instances
        .iter()
        .find(|instance| instance.key().target() == implementation)
        .expect("concrete inherited default instance");
    let [owner_argument] = implementation.key().type_arguments() else {
        panic!("Derived default has one concrete owner argument");
    };
    let Some(UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::List,
        arguments,
    }) = typed.types().types().get(*owner_argument)
    else {
        panic!("inherited owner argument must be List<Int>");
    };
    assert!(matches!(
        arguments.as_slice(),
        [argument]
            if matches!(
                typed.types().types().get(*argument),
                Some(UnitTypeKind::Builtin(BuiltinType::Int))
            )
    ));
    let static_self = implementation
        .key()
        .static_self()
        .expect("inherited default retains concrete StaticSelf");
    assert!(matches!(
        typed.types().types().get(static_self),
        Some(UnitTypeKind::Nominal {
            declaration: owner,
            arguments,
        }) if *owner == declaration(&names, "Host")
            && matches!(
                arguments.as_slice(),
                [argument]
                    if matches!(
                        typed.types().types().get(*argument),
                        Some(UnitTypeKind::Builtin(BuiltinType::Int))
                    )
            )
    ));
    assert!(
        instances
            .iter()
            .all(|instance| instance.key().target() != requirement)
    );
}
