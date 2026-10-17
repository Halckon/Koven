//! Recover only identities already present in the ordinary emitted module.
use super::super::super::declarations::FunctionDeclaration;
use super::*;

pub(super) fn map_type(
    module: &Module,
    typed: &TypedFile,
    ty: TypeId,
    mapped: &mut BTreeMap<TypeId, SsaTypeId>,
) -> Option<SsaTypeId> {
    if let Some(&id) = mapped.get(&ty) {
        return Some(id);
    }
    let kind = match typed.types().get(ty)? {
        TypeKind::Builtin(BuiltinType::Int) => SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        },
        TypeKind::Builtin(BuiltinType::Boolean) => SsaTypeKind::Boolean,
        TypeKind::Builtin(BuiltinType::String) => SsaTypeKind::StringOwner,
        TypeKind::Builtin(BuiltinType::Unit) => SsaTypeKind::Unit,
        TypeKind::Intrinsic {
            constructor,
            arguments,
        } => {
            let kind = match constructor {
                IntrinsicTypeConstructor::Map => MapContainerKind::Map,
                IntrinsicTypeConstructor::MutableMap => MapContainerKind::MutableMap,
                _ => return None,
            };
            let [key, value] = arguments.as_slice() else {
                panic!("real Map K/V")
            };
            SsaTypeKind::MapContainer {
                kind,
                key: map_type(module, typed, *key, mapped).unwrap(),
                value: map_type(module, typed, *value, mapped).unwrap(),
            }
        }
        TypeKind::Nullable(inner) => {
            let inner = map_type(module, typed, *inner, mapped).unwrap();
            let wrappers = module
                .nullable_maps
                .iter()
                .filter_map(|(&wrapper, &target)| (target == inner).then_some(wrapper))
                .collect::<Vec<_>>();
            let [wrapper] = wrappers.as_slice() else {
                panic!("one existing exact-inner N")
            };
            assert_eq!(module.aggregate_fields(*wrapper).unwrap()[1], inner);
            assert!(!module.map_results.contains_key(wrapper));
            mapped.insert(ty, *wrapper);
            return Some(*wrapper);
        }
        // Function names are mapped from their complete emitted ABI below.
        TypeKind::Function { .. } => return None,
        _ => return None,
    };
    let id = *module
        .type_ids
        .get(&kind)
        .expect("no helper type registration");
    mapped.insert(ty, id);
    Some(id)
}

pub(super) fn plans(
    module: &Module,
    names: &NameResolution,
    typed: &TypedFile,
    declarations: &[FunctionDeclaration],
    instances: &instances::FunctionInstancePlan,
    function_ids: &BTreeMap<FunctionInstanceKey, FunctionId>,
    type_ids: &BTreeMap<TypeId, SsaTypeId>,
) -> (BTreeMap<FunctionId, Plan>, source_closure::CallableLayouts) {
    let mut plans = BTreeMap::new();
    let mut layouts = source_closure::CallableLayouts::default();
    assert_eq!(instances.instances().len(), function_ids.len());
    assert_eq!(module.functions.len(), function_ids.len());
    for instance in instances.instances() {
        let declaration = &declarations[instance.template_index];
        let key = instances.source(instance.source).unwrap();
        assert_eq!(key, &instance.key);
        assert_eq!(key.symbol(), declaration.symbol);
        assert!(key.type_arguments().is_empty());
        assert!(key.callable_arguments().is_empty());
        assert!(instance.substitutions.is_empty());
        let function = module.function(function_ids[key]).unwrap();
        assert_eq!(function.origin, Origin::Source(declaration.span));
        assert_eq!(function.name, names.symbols()[key.symbol().index()].name());
        assert!(function.receiver.is_none());
        assert!(function.borrow_return.is_none());
        assert!(function.carrier_return.is_none());
        assert!(declaration.callable.owner().is_none());
        assert!(declaration.callable.borrow_return().is_none());
        assert!(matches!(
            declaration.callable.result_source(),
            lang_frontend::type_checking::CallableResultSource::Owned
        ));
        let return_type = declaration.callable.return_type();
        let returns = if builtin_type(typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![type_ids[&return_type]]
        };
        assert_eq!(function.return_types, returns);
        let parameters = &function
            .block(function.entry_block().unwrap())
            .unwrap()
            .parameters;
        assert_eq!(parameters.len(), declaration.callable.parameters().len());
        let mut entry_bindings = BTreeMap::new();
        let mut parameter_types = Vec::new();
        for ((parameter, symbol), &entity) in declaration
            .callable
            .parameters()
            .iter()
            .zip(declaration.callable.parameter_symbols())
            .zip(parameters)
        {
            assert_eq!(parameter.mode, ParameterMode::Value);
            let symbol = symbol.expect("real parameter symbol");
            let ty = EntityType::Value(type_ids[&parameter.ty]);
            assert_eq!(function.entity(entity).unwrap().ty, ty);
            let EntityId::Value(value) = entity else {
                panic!("ordinary own parameter")
            };
            entry_bindings.insert(symbol, LoweredValue::Value(value));
            parameter_types.push(ty);
        }
        let pointer = SsaTypeKind::FunctionPointer {
            signature: CallableSignature {
                parameters: parameter_types,
                returns,
            },
        };
        let pointer = *module
            .type_ids
            .get(&pointer)
            .expect("ordinary predeclared callable ABI");
        layouts.named.insert(instance.source, pointer);
        let Item::Function { form, .. } = &declaration.item else {
            panic!("source function")
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => panic!("bounded B/C fixtures have block functions"),
        };
        assert!(
            plans
                .insert(
                    function.id,
                    Plan {
                        source: instance.source,
                        body,
                        return_type,
                        entry_bindings
                    }
                )
                .is_none()
        );
    }
    (plans, layouts)
}
