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
    for (&range, &endpoint) in &module.integer_ranges {
        if !module.valid_integer_range_type(range, endpoint) {
            push_type_error(
                errors,
                range,
                "integer Range requires matching signed Int/Long endpoints and Boolean emptiness",
            );
        }
    }
    for (&result, &value) in &module.map_results {
        if !module.valid_map_result_type(result, value) {
            push_type_error(
                errors,
                result,
                "Map result requires Boolean presence and matching payload ownership",
            );
        }
    }
    let mut nullable_map_inners = BTreeSet::new();
    for (&nullable, &inner) in &module.nullable_maps {
        if !module.valid_nullable_map_type(nullable, inner)
            || module.map_results.contains_key(&nullable)
            || !nullable_map_inners.insert(inner)
        {
            push_type_error(
                errors,
                nullable,
                "nullable Map requires unique local MoveOnly Map storage, disjoint from Map results",
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
    let is_view = |ty| matches!(module.type_kind(ty), Some(SsaTypeKind::CallableView { .. }));
    let embeds_view = match kind {
        SsaTypeKind::Aggregate { fields, .. } => fields.iter().copied().any(is_view),
        SsaTypeKind::TaggedUnion { variants, .. } => variants.iter().copied().any(is_view),
        SsaTypeKind::SequentialContainer { element, .. } => is_view(*element),
        SsaTypeKind::MapContainer { key, value, .. } => is_view(*key) || is_view(*value),
        SsaTypeKind::NullableHandle { inner } | SsaTypeKind::InlineNullable { inner, .. } => {
            is_view(*inner)
        }
        SsaTypeKind::HeapOwner {
            payload: Some(target),
            ..
        }
        | SsaTypeKind::SharedOwner {
            payload: Some(target),
            ..
        } => is_view(*target),
        // A borrowed closure may share-capture a view (its environment holds a reference to the
        // descriptor); only owned captures would store the view itself.
        SsaTypeKind::ConcreteClosure { captures, .. } => captures.iter().any(|capture| {
            capture.mode == super::model::ClosureCaptureMode::Owned && is_view(capture.ty)
        }),
        _ => false,
    };
    if embeds_view {
        push_type_error(
            errors,
            id,
            "a callable view cannot be stored in a field, payload, container or capture",
        );
    }
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
        SsaTypeKind::InlineNullable { inner, niche } => {
            if inner.module() != module.id
                || super::types::inline_nullable_niche(module, *inner) != Some(*niche)
            {
                push_type_error(
                    errors,
                    id,
                    "inline nullable inner type must be a defined local inline type with matching niche",
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
        SsaTypeKind::FunctionPointer { signature } | SsaTypeKind::CallableView { signature } => {
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
    if signature
        .returns
        .iter()
        .any(|ty| module.callable_view_signature(*ty).is_some())
        || signature.parameters.iter().any(|parameter| {
            !matches!(
                parameter,
                super::model::EntityType::Loan {
                    kind: super::model::LoanKind::Shared,
                    ..
                }
            ) && module
                .callable_view_signature(parameter.semantic_type())
                .is_some()
        })
    {
        push_type_error(
            errors,
            id,
            "a callable view crosses a signature only as a shared loan parameter",
        );
    }
    if signature.returns.len() > 1 {
        push_type_error(
            errors,
            id,
            "callable signature supports at most one return type",
        );
    }
    if signature.never_returns && !signature.returns.is_empty() {
        push_type_error(
            errors,
            id,
            "a non-returning callable cannot declare a result",
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

pub(in crate::ssa) mod module {
    //! Module validation and shared proofs for function verification.

    use std::collections::VecDeque;

    use crate::ssa::{
        model::{Block, Edge, EntityData, EntityId, EntityType, Function, LoanKind, Module},
        verify::*,
        verify_ownership::edges,
    };

    pub(in crate::ssa) fn verify_module(module: &Module, errors: &mut Vec<VerifyError>) {
        let module_before = errors.len();
        crate::ssa::verify_types::verify_types(module, errors);
        verify_deinits(module, errors);
        let capture_types =
            (errors.len() == module_before).then(|| closure_escape::TypeCaptures::compute(module));

        let trusted_view_fields = closure_content::trusted_view_fields(module);

        for (function_index, function) in module.functions.iter().enumerate() {
            if function.id.module() != module.id || function.id.index() != function_index {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::InvalidFunctionId,
                    location: VerifyLocation::Function(function.id),
                    origin: Some(function.origin.clone()),
                });
                continue;
            }
            let before = errors.len();
            verify_function_structure(module, function, errors);
            if errors.len() == before {
                verify_cfg_types(module, function, errors);
                verify_dominance(function, errors);
                // Ownership content proofs require a validated module type/deinit graph.
                if errors.len() == before
                    && let Some(types) = &capture_types
                {
                    crate::ssa::verify_ownership::verify_ownership(
                        module,
                        function,
                        types,
                        trusted_view_fields.get(&function.id),
                        errors,
                    );
                    if errors.len() == before {
                        closure_escape::verify(function, types, errors);
                    }
                }
            }
        }
    }

    fn verify_deinits(module: &Module, errors: &mut Vec<VerifyError>) {
        for (&owner, &function) in &module.deinits {
            let valid = crate::ssa::types::valid_deinit_signature(module, owner, function)
                && module.function(function).is_some_and(|function| {
                    function.blocks.first().is_some_and(|entry| {
                        entry.parameters.len() == 1
                            && function
                                .entity(entry.parameters[0])
                                .is_some_and(|parameter| Some(parameter.ty) == function.receiver)
                    })
                });
            if !valid {
                errors.push(VerifyError {
                    kind: VerifyErrorKind::InvalidTypeDefinition {
                        reason: "deinit requires one exact shared heap-owner receiver and a Unit result",
                    },
                    location: VerifyLocation::Type(owner),
                    origin: module
                        .function(function)
                        .map(|function| function.origin.clone()),
                });
            }
        }
    }

    pub(in crate::ssa) fn verify_callable_view_entity(
        module: &Module,
        entity: EntityId,
        data: &EntityData,
        errors: &mut Vec<VerifyError>,
    ) {
        let misplaced = match data.ty {
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            } => false,
            other => module
                .callable_view_signature(other.semantic_type())
                .is_some(),
        };
        if misplaced {
            errors.push(entity_error(
                entity,
                data.origin.clone(),
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "a callable view is only a shared loan target",
                },
            ));
        }
    }

    pub(in crate::ssa) fn verify_callable_view_returns(
        module: &Module,
        function: &Function,
        errors: &mut Vec<VerifyError>,
    ) {
        if function
            .return_types
            .iter()
            .any(|ty| module.callable_view_signature(*ty).is_some())
        {
            errors.push(function_error(
                function,
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "a callable view cannot be returned",
                },
            ));
        }
    }

    /// The caller supplies domain transfer, simultaneous edge transport, and its monotone join.
    /// Structural verification must have succeeded before walking the CFG.
    pub(in crate::ssa) fn block_entries<State: Clone>(
        function: &Function,
        defaults: &State,
        transfer: impl Fn(&Block, &mut State),
        transport: impl Fn(&Edge, &State) -> State,
        join: impl Fn(&mut State, State) -> bool,
    ) -> Vec<State> {
        let mut reachable = vec![false; function.blocks.len()];
        let mut pending = VecDeque::from([0]);
        reachable[0] = true;
        while let Some(index) = pending.pop_front() {
            for edge in edges(
                &function.blocks[index]
                    .terminator
                    .as_ref()
                    .expect("verified terminator")
                    .kind,
            ) {
                if !reachable[edge.target.index()] {
                    reachable[edge.target.index()] = true;
                    pending.push_back(edge.target.index());
                }
            }
        }
        let mut entries = vec![None; function.blocks.len()];
        let mut queued = vec![false; function.blocks.len()];
        for index in 0..function.blocks.len() {
            // Unreachable components have no caller proof, including closed CFG cycles.
            if index == 0 || !reachable[index] {
                entries[index] = Some(defaults.clone());
                queued[index] = true;
                pending.push_back(index);
            }
        }
        while let Some(index) = pending.pop_front() {
            queued[index] = false;
            let mut state = entries[index]
                .as_ref()
                .expect("queued entry exists")
                .clone();
            let block = &function.blocks[index];
            transfer(block, &mut state);
            for edge in edges(&block.terminator.as_ref().expect("verified terminator").kind) {
                let incoming = transport(edge, &state);
                let changed = match &mut entries[edge.target.index()] {
                    None => {
                        entries[edge.target.index()] = Some(incoming);
                        true
                    }
                    Some(previous) => join(previous, incoming),
                };
                if changed && !queued[edge.target.index()] {
                    queued[edge.target.index()] = true;
                    pending.push_back(edge.target.index());
                }
            }
        }
        entries
            .into_iter()
            .map(|entry| entry.expect("all components were seeded"))
            .collect()
    }
}

#[cfg(test)]
pub(in crate::ssa) mod render_types {
    //! SSA type and Map operation rendering for the deterministic debug renderer.
    use std::fmt::{self, Write};

    use crate::ssa::{
        model::{ClosureCaptureMode, EntityId, Operation, SsaTypeKind},
        render::{write_callable_signature, write_entity_id, write_type_id, write_type_ids},
    };

    pub(in crate::ssa) mod map {
        use super::*;

        pub(in crate::ssa) fn write_operation(
            output: &mut String,
            operation: &Operation,
        ) -> fmt::Result {
            match operation {
                Operation::MapIterationFirst { source } => {
                    write!(output, "map.iteration.first %l{}", source.index())
                }
                Operation::MapIterationHasNext { source, cursor }
                | Operation::MapIterationNext { source, cursor }
                | Operation::MapIterationKeyLoan { source, cursor }
                | Operation::MapIterationValueLoan { source, cursor } => {
                    let name = match operation {
                        Operation::MapIterationHasNext { .. } => "has_next",
                        Operation::MapIterationNext { .. } => "next",
                        Operation::MapIterationKeyLoan { .. } => "key_loan",
                        Operation::MapIterationValueLoan { .. } => "value_loan",
                        _ => unreachable!("matched Map iteration operation"),
                    };
                    write!(
                        output,
                        "map.iteration.{name} %l{}, %v{}",
                        source.index(),
                        cursor.index()
                    )
                }
                Operation::MapConsume { owner, map_type } => {
                    output.write_str("map.consume ")?;
                    write_entity_id(output, EntityId::Value(*owner))?;
                    output.write_str(", ")?;
                    write_type_id(output, *map_type)
                }
                Operation::MapConstruct { map_type } => {
                    output.write_str("map.construct ")?;
                    write_type_id(output, *map_type)
                }
                Operation::MapSize { owner } => {
                    output.write_str("map.size ")?;
                    write_entity_id(output, *owner)
                }
                Operation::MapContains { owner, key } => {
                    output.write_str("map.contains ")?;
                    write_entity_id(output, *owner)?;
                    output.write_str(", ")?;
                    write_entity_id(output, *key)
                }
                Operation::MapRequireValue { source, key } => {
                    write!(output, "map.require_value %l{}, ", source.index())?;
                    write_entity_id(output, *key)
                }
                Operation::MapWithValue {
                    source,
                    key,
                    action,
                } => {
                    write!(output, "map.with_value %l{}, ", source.index())?;
                    write_entity_id(output, *key)?;
                    write!(output, ", %l{}", action.index())
                }
                Operation::MapGet { owner, key } => {
                    output.write_str("map.get ")?;
                    write_entity_id(output, *owner)?;
                    output.write_str(", ")?;
                    write_entity_id(output, *key)
                }
                Operation::MapResultUnwrap { result } => {
                    output.write_str("map.result.unwrap ")?;
                    write_entity_id(output, EntityId::Value(*result))
                }
                Operation::MapPut { owner, key, value } => {
                    output.write_str("map.put ")?;
                    write_entity_id(output, EntityId::Value(*owner))?;
                    output.write_str(", ")?;
                    write_entity_id(output, EntityId::Value(*key))?;
                    output.write_str(", ")?;
                    write_entity_id(output, EntityId::Value(*value))
                }
                Operation::MapRemove { owner, key } => {
                    output.write_str("map.remove ")?;
                    write_entity_id(output, EntityId::Value(*owner))?;
                    output.write_str(", ")?;
                    write_entity_id(output, *key)
                }
                _ => Err(fmt::Error),
            }
        }
    }

    pub(in crate::ssa) fn write_type_kind(output: &mut String, kind: &SsaTypeKind) -> fmt::Result {
        match kind {
            SsaTypeKind::Unit => output.write_str("unit"),
            SsaTypeKind::Boolean => output.write_str("bool"),
            SsaTypeKind::Char => output.write_str("char"),
            SsaTypeKind::Integer { bits, signed } => {
                write!(output, "{}{bits}", if *signed { 'i' } else { 'u' })
            }
            SsaTypeKind::Opaque { name, ownership } => {
                write!(output, "opaque {name:?} {:?}", ownership)
            }
            SsaTypeKind::ZeroSized { name, ownership } => {
                write!(output, "zst {name:?} {:?}", ownership)
            }
            SsaTypeKind::Aggregate {
                name,
                fields,
                ownership,
            } => {
                write!(output, "aggregate {name:?} {:?} (", ownership)?;
                write_type_ids(output, fields)?;
                output.push(')');
                Ok(())
            }
            SsaTypeKind::TaggedUnion {
                name,
                variants,
                ownership,
            } => {
                write!(output, "tagged_union {name:?} {:?} (", ownership)?;
                write_type_ids(output, variants)?;
                output.push(')');
                Ok(())
            }
            SsaTypeKind::HeapOwner { name, payload } => {
                write!(output, "heap_owner {name:?}")?;
                if let Some(payload) = payload {
                    output.write_str(" payload ")?;
                    write_type_id(output, *payload)?;
                } else {
                    output.write_str(" <declared>")?;
                }
                Ok(())
            }
            SsaTypeKind::SharedOwner { name, payload } => {
                write!(output, "shared_owner {name:?}")?;
                if let Some(payload) = payload {
                    output.write_str(" payload ")?;
                    write_type_id(output, *payload)?;
                } else {
                    output.write_str(" <declared>")?;
                }
                Ok(())
            }
            SsaTypeKind::StringOwner => output.write_str("string_owner"),
            SsaTypeKind::NullableHandle { inner } => {
                output.write_str("nullable_handle<")?;
                write_type_id(output, *inner)?;
                output.push('>');
                Ok(())
            }
            SsaTypeKind::InlineNullable { inner, niche } => {
                let form = if *niche { "niche" } else { "inline" };
                write!(output, "nullable_{form}<")?;
                write_type_id(output, *inner)?;
                output.push('>');
                Ok(())
            }
            SsaTypeKind::SequentialContainer { kind, element } => {
                write!(output, "container {kind:?}<")?;
                write_type_id(output, *element)?;
                output.push('>');
                Ok(())
            }
            SsaTypeKind::RangeView { source } => {
                output.write_str("range_view<")?;
                write_type_id(output, *source)?;
                output.write_str(">")
            }
            SsaTypeKind::MapContainer { kind, key, value } => {
                write!(output, "map {kind:?}<")?;
                write_type_id(output, *key)?;
                output.write_str(", ")?;
                write_type_id(output, *value)?;
                output.push('>');
                Ok(())
            }
            SsaTypeKind::SharedReference { target } => {
                output.write_str("shared_ref<")?;
                write_type_id(output, *target)?;
                output.push('>');
                Ok(())
            }
            SsaTypeKind::FunctionPointer { signature } => {
                output.write_str("function_pointer ")?;
                write_callable_signature(output, signature)
            }
            SsaTypeKind::CallableView { signature } => {
                output.write_str("callable_view ")?;
                write_callable_signature(output, signature)
            }
            SsaTypeKind::ConcreteClosure {
                name,
                signature,
                environment,
                captures,
            } => {
                write!(output, "closure {name:?} env ")?;
                write_type_id(output, *environment)?;
                output.write_str(" captures (")?;
                for (index, capture) in captures.iter().enumerate() {
                    if index != 0 {
                        output.write_str(", ")?;
                    }
                    output.write_str(match capture.mode {
                        ClosureCaptureMode::Shared => "shared ",
                        ClosureCaptureMode::Owned => "owned ",
                    })?;
                    write_type_id(output, capture.ty)?;
                }
                output.write_str(") ")?;
                write_callable_signature(output, signature)
            }
        }
    }
}
