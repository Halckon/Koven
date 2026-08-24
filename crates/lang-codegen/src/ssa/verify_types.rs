//! Named SSA type graph validation.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    model::{Module, Ownership, SsaTypeId, SsaTypeKind},
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
}

fn verify_named_identity(
    module: &Module,
    id: SsaTypeId,
    kind: &SsaTypeKind,
    named_types: &mut BTreeMap<String, SsaTypeId>,
    errors: &mut Vec<VerifyError>,
) {
    let name = match kind {
        SsaTypeKind::Aggregate { name, .. } | SsaTypeKind::HeapOwner { name, .. } => name,
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
        SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        } => {
            if payload.module() != module.id
                || !matches!(
                    module.type_kind(*payload),
                    Some(SsaTypeKind::Aggregate { .. })
                )
            {
                push_type_error(
                    errors,
                    id,
                    "heap owner payload must be a local aggregate type",
                );
            }
        }
        SsaTypeKind::HeapOwner { payload: None, .. } => {
            push_type_error(errors, id, "heap owner declaration must be defined");
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
        SsaTypeKind::Unit
        | SsaTypeKind::Boolean
        | SsaTypeKind::Integer { .. }
        | SsaTypeKind::Opaque { .. } => {}
    }
}

fn verify_inline_type_cycles(module: &Module, errors: &mut Vec<VerifyError>) {
    for (index, kind) in module.types.iter().enumerate() {
        if !matches!(kind, SsaTypeKind::Aggregate { .. }) {
            continue;
        }
        let id = SsaTypeId {
            module: module.id,
            index,
        };
        if aggregate_reaches(module, id, id, &mut BTreeSet::new()) {
            push_type_error(errors, id, "aggregate fields must not form an inline cycle");
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

fn aggregate_reaches(
    module: &Module,
    current: SsaTypeId,
    target: SsaTypeId,
    active: &mut BTreeSet<SsaTypeId>,
) -> bool {
    let Some(SsaTypeKind::Aggregate { fields, .. }) = module.type_kind(current) else {
        return false;
    };
    if !active.insert(current) {
        return current == target;
    }
    let reaches = fields.iter().copied().any(|field| {
        field == target
            || matches!(module.type_kind(field), Some(SsaTypeKind::Aggregate { .. }))
                && aggregate_reaches(module, field, target, active)
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
