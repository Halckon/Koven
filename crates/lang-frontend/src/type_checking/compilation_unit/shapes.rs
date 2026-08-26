use std::collections::BTreeMap;

use crate::{
    name_resolution::{DeclarationId, UnitSymbolId},
    source::Span,
    type_checking::{BuiltinType, Capability, IntrinsicTypeConstructor},
};

use super::{
    UnitCallableSignature, UnitDeclarationSignature, UnitTypeId, UnitTypeKind, UnitTypeTable,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ShapeType {
    Builtin(BuiltinType),
    Nullable(Box<Self>),
    Function(bool, Vec<Self>, Box<Self>),
    Nominal(DeclarationId, Vec<Self>),
    Intrinsic(IntrinsicTypeConstructor, Vec<Self>),
    Parameter(usize),
    OuterParameter(UnitSymbolId),
    Capability(Capability),
    Deferred,
    Error,
}

pub(super) fn duplicate_top_level_shapes(
    declarations: &[UnitDeclarationSignature],
    types: &UnitTypeTable,
) -> Vec<(Span, Span)> {
    let mut first = BTreeMap::new();
    let mut duplicates = Vec::new();
    for signature in declarations {
        let Some(callable) = signature.callable() else {
            continue;
        };
        let key = (
            signature.package(),
            callable.name().to_owned(),
            callable.type_parameters().len(),
            callable_shape(types, callable),
        );
        if let Some(first_span) = first.get(&key).copied() {
            duplicates.push((callable.name_span(), first_span));
        } else {
            first.insert(key, callable.name_span());
        }
    }
    duplicates
}

pub(super) fn duplicate_member_shapes(
    members: &[UnitCallableSignature],
    types: &UnitTypeTable,
) -> Vec<(Span, Span)> {
    let mut first = BTreeMap::new();
    let mut duplicates = Vec::new();
    for member in members {
        let key = (
            member.name().to_owned(),
            member.type_parameters().len(),
            callable_shape(types, member),
        );
        if let Some(first_span) = first.get(&key).copied() {
            duplicates.push((member.name_span(), first_span));
        } else {
            first.insert(key, member.name_span());
        }
    }
    duplicates
}

fn callable_shape(types: &UnitTypeTable, callable: &UnitCallableSignature) -> Vec<ShapeType> {
    let parameters = callable
        .type_parameters()
        .iter()
        .enumerate()
        .map(|(index, symbol)| (*symbol, index))
        .collect::<BTreeMap<_, _>>();
    callable
        .parameters()
        .iter()
        .map(|parameter| shape_type(types, parameter.ty(), &parameters))
        .collect()
}

fn shape_type(
    types: &UnitTypeTable,
    ty: UnitTypeId,
    parameters: &BTreeMap<UnitSymbolId, usize>,
) -> ShapeType {
    match types.get(ty) {
        Some(UnitTypeKind::Builtin(builtin)) => ShapeType::Builtin(*builtin),
        Some(UnitTypeKind::Nullable(inner)) => {
            ShapeType::Nullable(Box::new(shape_type(types, *inner, parameters)))
        }
        Some(UnitTypeKind::Function {
            move_only,
            parameters: function_parameters,
            return_type,
        }) => ShapeType::Function(
            *move_only,
            function_parameters
                .iter()
                .map(|parameter| shape_type(types, parameter.ty(), parameters))
                .collect(),
            Box::new(shape_type(types, *return_type, parameters)),
        ),
        Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) => ShapeType::Nominal(
            *declaration,
            arguments
                .iter()
                .map(|argument| shape_type(types, *argument, parameters))
                .collect(),
        ),
        Some(UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        }) => ShapeType::Intrinsic(
            *constructor,
            arguments
                .iter()
                .map(|argument| shape_type(types, *argument, parameters))
                .collect(),
        ),
        Some(UnitTypeKind::TypeParameter(symbol)) => parameters
            .get(symbol)
            .copied()
            .map_or(ShapeType::OuterParameter(*symbol), ShapeType::Parameter),
        Some(UnitTypeKind::Capability(capability)) => ShapeType::Capability(*capability),
        Some(UnitTypeKind::Deferred(_)) | Some(UnitTypeKind::StaticSelf(_)) => ShapeType::Deferred,
        Some(UnitTypeKind::EnumCase { root, .. }) => shape_type(types, *root, parameters),
        Some(UnitTypeKind::Error) | None => ShapeType::Error,
    }
}
