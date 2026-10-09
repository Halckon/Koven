//! Named SSA type graph validation.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    model::{CallableSignature, ClosureCaptureMode, Module, Ownership, SsaTypeId, SsaTypeKind},
    verify::{VerifyError, VerifyErrorKind, VerifyLocation},
};

pub(super) fn verify_types(module: &Module, errors: &mut Vec<VerifyError>) {
    let mut named_types = BTreeMap::new();
    for (index, kind) in module.types.iter().enumerate() {
        let id = SsaTypeId {
            module: module.id,
            index,
        };
        if let SsaTypeKind::Integer { bits, .. } = kind
            && !(1..=128).contains(bits)
        {
            push_type_error(
                errors,
                id,
                "integer type width must be between 1 and 128 bits",
            );
        }
        if module.type_kind(id) != Some(kind) {
            errors.push(VerifyError {
                kind: VerifyErrorKind::UnknownType(id),
                location: VerifyLocation::Module(module.id),
                origin: None,
            });
        }
        verify_named_identity(module, id, kind, &mut named_types, errors);
        verify_type_definition(module, id, kind, errors);
    }
    verify_inline_type_cycles(module, errors);
    verify_container_type_cycles(module, errors);
    for (&result, &value) in &module.map_results {
        if !module.valid_map_result_type(result, value) {
            push_type_error(
                errors,
                result,
                "Map result requires Boolean presence and matching payload ownership",
            );
        }
    }
}

fn verify_named_identity(
    module: &Module,
    id: SsaTypeId,
    kind: &SsaTypeKind,
    named_types: &mut BTreeMap<String, SsaTypeId>,
    errors: &mut Vec<VerifyError>,
) {
    let name = match kind {
        SsaTypeKind::Aggregate { name, .. }
        | SsaTypeKind::TaggedUnion { name, .. }
        | SsaTypeKind::HeapOwner { name, .. }
        | SsaTypeKind::SharedOwner { name, .. }
        | SsaTypeKind::ConcreteClosure { name, .. } => name,
        _ => return,
    };
    if name.is_empty() {
        push_type_error(errors, id, "named type name must not be empty");
    }
    if named_types.insert(name.clone(), id).is_some() {
        push_type_error(errors, id, "named type name must be unique in its module");
    }
    if module.named_type_ids.get(name) != Some(&id) {
        push_type_error(
            errors,
            id,
            "named type identity must match the module name index",
        );
    }
}

fn verify_type_definition(
    module: &Module,
    id: SsaTypeId,
    kind: &SsaTypeKind,
    errors: &mut Vec<VerifyError>,
) {
    let is_range = |ty| matches!(module.type_kind(ty), Some(SsaTypeKind::RangeView { .. }));
    let embeds_range = match kind {
        SsaTypeKind::Aggregate { fields, .. } => fields.iter().copied().any(is_range),
        SsaTypeKind::SequentialContainer { element, .. } => is_range(*element),
        SsaTypeKind::MapContainer { key, value, .. } => is_range(*key) || is_range(*value),
        SsaTypeKind::SharedReference { target }
        | SsaTypeKind::SharedOwner {
            payload: Some(target),
            ..
        } => is_range(*target),
        SsaTypeKind::ConcreteClosure { captures, .. } => {
            captures.iter().any(|capture| is_range(capture.ty))
        }
        _ => false,
    };
    if embeds_range {
        push_type_error(
            errors,
            id,
            "range metadata cannot be stored in an owning field, container or capture",
        );
    }
    match kind {
        SsaTypeKind::Aggregate {
            fields, ownership, ..
        } => {
            let derived = if fields
                .iter()
                .any(|field| module.type_ownership(*field) == Some(Ownership::MoveOnly))
            {
                Ownership::MoveOnly
            } else {
                Ownership::Copyable
            };
            if *ownership != derived {
                push_type_error(
                    errors,
                    id,
                    "aggregate ownership must be derived from all fields",
                );
            }
            for field in fields {
                if field.module() != module.id || module.type_kind(*field).is_none() {
                    push_type_error(errors, id, "field type must exist in the same module");
                }
            }
        }
        SsaTypeKind::TaggedUnion {
            variants,
            ownership,
            ..
        } => {
            let derived = if variants
                .iter()
                .any(|variant| module.type_ownership(*variant) == Some(Ownership::MoveOnly))
            {
                Ownership::MoveOnly
            } else {
                Ownership::Copyable
            };
            if variants.is_empty() || *ownership != derived {
                push_type_error(
                    errors,
                    id,
                    "tagged union must have variants and derive ownership from payloads",
                );
            }
            for variant in variants {
                if variant.module() != module.id
                    || !matches!(
                        module.type_kind(*variant),
                        Some(SsaTypeKind::Aggregate { .. })
                    )
                {
                    push_type_error(errors, id, "tagged union payloads must be local aggregates");
                }
            }
        }
        SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        } => {
            if payload.module() != module.id
                || !matches!(
                    module.type_kind(*payload),
                    Some(SsaTypeKind::Aggregate { .. } | SsaTypeKind::TaggedUnion { .. })
                )
            {
                push_type_error(
                    errors,
                    id,
                    "heap owner payload must be a local aggregate or tagged union type",
                );
            }
        }
        SsaTypeKind::HeapOwner { payload: None, .. } => {
            push_type_error(errors, id, "heap owner declaration must be defined");
        }
        SsaTypeKind::SharedOwner {
            payload: Some(payload),
            ..
        } => {
            if payload.module() != module.id
                || module.type_kind(*payload).is_none()
                || !module.type_is_defined(*payload)
            {
                push_type_error(
                    errors,
                    id,
                    "shared owner payload must be a defined local type",
                );
            }
        }
        SsaTypeKind::SharedOwner { payload: None, .. } => {
            push_type_error(errors, id, "shared owner declaration must be defined");
        }
        SsaTypeKind::NullableHandle { inner } => {
            if inner.module() != module.id
                || !module.type_is_defined(*inner)
                || !matches!(
                    module.type_kind(*inner),
                    Some(SsaTypeKind::HeapOwner { .. } | SsaTypeKind::SharedOwner { .. })
                )
            {
                push_type_error(
                    errors,
                    id,
                    "nullable handle inner type must be a defined local pointer-like owner",
                );
            }
        }
        SsaTypeKind::RangeView { source } => {
            if !matches!(
                module.type_kind(*source),
                Some(SsaTypeKind::SequentialContainer {
                    kind: super::model::SequentialContainerKind::List,
                    ..
                })
            ) {
                push_type_error(errors, id, "range source must be a defined local List");
            }
        }
        SsaTypeKind::SequentialContainer { element, .. } => {
            if element.module() != module.id
                || module.type_kind(*element).is_none()
                || !module.type_is_defined(*element)
            {
                push_type_error(
                    errors,
                    id,
                    "sequential container element type must be defined in the same module",
                );
            }
        }
        SsaTypeKind::MapContainer { key, value, .. } => {
            if key.module() != module.id
                || module.type_kind(*key).is_none()
                || !module.type_is_defined(*key)
            {
                push_type_error(
                    errors,
                    id,
                    "map container key type must be defined in the same module",
                );
            }
            if !matches!(
                module.type_kind(*key),
                Some(
                    SsaTypeKind::StringOwner
                        | SsaTypeKind::Boolean
                        | SsaTypeKind::Char
                        | SsaTypeKind::Integer {
                            bits: 32,
                            signed: true
                        }
                )
            ) {
                push_type_error(
                    errors,
                    id,
                    "map key must have a supported Hashable representation",
                );
            }
            if value.module() != module.id
                || module.type_kind(*value).is_none()
                || !module.type_is_defined(*value)
            {
                push_type_error(
                    errors,
                    id,
                    "map container value type must be defined in the same module",
                );
            }
            if matches!(
                module.type_kind(*value),
                Some(SsaTypeKind::SharedReference { .. })
            ) {
                push_type_error(errors, id, "map value must own storable data");
            }
        }
        SsaTypeKind::SharedReference { target } => {
            if target.module() != module.id
                || module.type_kind(*target).is_none()
                || !module.type_is_defined(*target)
            {
                push_type_error(
                    errors,
                    id,
                    "shared reference target must be defined in the same module",
                );
            }
        }
        SsaTypeKind::FunctionPointer { signature } => {
            verify_callable_signature(module, id, signature, errors);
        }
        SsaTypeKind::ConcreteClosure {
            signature,
            environment,
            captures,
            ..
        } => {
            verify_callable_signature(module, id, signature, errors);
            if captures.is_empty() {
                push_type_error(
                    errors,
                    id,
                    "concrete closure must have at least one capture",
                );
            }
            let Some(fields) = module.aggregate_fields(*environment) else {
                push_type_error(errors, id, "closure environment must be a local aggregate");
                return;
            };
            if fields.len() != captures.len() {
                push_type_error(errors, id, "closure environment must match capture slots");
            } else {
                for (field, capture) in fields.iter().zip(captures) {
                    let expected = match capture.mode {
                        ClosureCaptureMode::Owned => Some(capture.ty),
                        ClosureCaptureMode::Shared => {
                            module.types.iter().enumerate().find_map(|(index, kind)| {
                                (*kind == SsaTypeKind::SharedReference { target: capture.ty })
                                    .then_some(SsaTypeId {
                                        module: module.id,
                                        index,
                                    })
                            })
                        }
                    };
                    if expected != Some(*field) {
                        push_type_error(errors, id, "closure environment must match capture slots");
                        break;
                    }
                }
            }
        }
        SsaTypeKind::Unit
        | SsaTypeKind::Boolean
        | SsaTypeKind::Char
        | SsaTypeKind::Integer { .. }
        | SsaTypeKind::Opaque { .. }
        | SsaTypeKind::ZeroSized { .. }
        | SsaTypeKind::StringOwner => {}
    }
}

fn verify_callable_signature(
    module: &Module,
    id: SsaTypeId,
    signature: &CallableSignature,
    errors: &mut Vec<VerifyError>,
) {
    if signature.returns.iter().any(|ty|matches!(module.type_kind(*ty),Some(SsaTypeKind::RangeView {..}))) || signature.parameters.iter().any(|p|matches!(p,super::model::EntityType::Value(ty) if matches!(module.type_kind(*ty),Some(SsaTypeKind::RangeView {..})))) {push_type_error(errors,id,"range ABI cannot be erased into an owned callable signature");}
    if signature.returns.len() > 1 {
        push_type_error(
            errors,
            id,
            "callable signature supports at most one return type",
        );
    }
    if signature
        .parameters
        .iter()
        .any(|parameter| matches!(parameter, super::model::EntityType::Place(_)))
    {
        push_type_error(errors, id, "callable signature parameters cannot be places");
    }
    for ty in signature
        .parameters
        .iter()
        .map(|parameter| parameter.semantic_type())
        .chain(signature.returns.iter().copied())
    {
        if ty.module() != module.id || module.type_kind(ty).is_none() || !module.type_is_defined(ty)
        {
            push_type_error(
                errors,
                id,
                "callable signature types must be defined in the same module",
            );
        }
    }
}

fn verify_inline_type_cycles(module: &Module, errors: &mut Vec<VerifyError>) {
    for (index, kind) in module.types.iter().enumerate() {
        if !matches!(
            kind,
            SsaTypeKind::Aggregate { .. }
                | SsaTypeKind::TaggedUnion { .. }
                | SsaTypeKind::ConcreteClosure { .. }
        ) {
            continue;
        }
        let id = SsaTypeId {
            module: module.id,
            index,
        };
        if inline_type_reaches(module, id, id, &mut BTreeSet::new()) {
            let reason = match kind {
                SsaTypeKind::Aggregate { .. } => "aggregate fields must not form an inline cycle",
                SsaTypeKind::TaggedUnion { .. } => {
                    "tagged union payloads must not form an inline cycle"
                }
                _ => "closure environment must not form an inline cycle",
            };
            push_type_error(errors, id, reason);
        }
    }
}

fn verify_container_type_cycles(module: &Module, errors: &mut Vec<VerifyError>) {
    for (index, kind) in module.types.iter().enumerate() {
        if !matches!(kind, SsaTypeKind::SequentialContainer { .. }) {
            continue;
        }
        let id = SsaTypeId {
            module: module.id,
            index,
        };
        let mut current = id;
        let mut visited = BTreeSet::new();
        while visited.insert(current) {
            let Some(SsaTypeKind::SequentialContainer { element, .. }) = module.type_kind(current)
            else {
                break;
            };
            current = *element;
        }
        if current == id {
            push_type_error(
                errors,
                id,
                "sequential container identity must not form a structural cycle",
            );
        }
    }
}

fn inline_type_reaches(
    module: &Module,
    current: SsaTypeId,
    target: SsaTypeId,
    active: &mut BTreeSet<SsaTypeId>,
) -> bool {
    let fields = match module.type_kind(current) {
        Some(SsaTypeKind::Aggregate { fields, .. }) => fields.as_slice(),
        Some(SsaTypeKind::TaggedUnion { variants, .. }) => variants.as_slice(),
        Some(SsaTypeKind::ConcreteClosure { environment, .. }) => std::slice::from_ref(environment),
        _ => return false,
    };
    if !active.insert(current) {
        return current == target;
    }
    let reaches = fields.iter().copied().any(|field| {
        field == target
            || matches!(
                module.type_kind(field),
                Some(
                    SsaTypeKind::Aggregate { .. }
                        | SsaTypeKind::TaggedUnion { .. }
                        | SsaTypeKind::ConcreteClosure { .. }
                )
            ) && inline_type_reaches(module, field, target, active)
    });
    active.remove(&current);
    reaches
}

fn push_type_error(errors: &mut Vec<VerifyError>, id: SsaTypeId, reason: &'static str) {
    errors.push(VerifyError {
        kind: VerifyErrorKind::InvalidTypeDefinition { reason },
        location: VerifyLocation::Type(id),
        origin: None,
    });
}
