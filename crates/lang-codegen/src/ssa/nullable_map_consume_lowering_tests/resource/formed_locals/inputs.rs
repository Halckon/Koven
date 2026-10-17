//! Formation-specific public typed facts; no Resource Case or synthetic capability.

use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, SourceUnitId, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicCallable, IntrinsicTypeConstructor, TypeId,
        TypeKind, TypedFile, UnitExpressionId, UnitTypeId, UnitTypeKind, UnitTypeRefId,
    },
};

use super::{
    super::{ParsedFile, SourceMap, Trace},
    sources::{Case, Initializer, Shape},
    syntax,
};

fn single_shape(typed: &TypedFile, ty: TypeId, shape: Shape) -> bool {
    let int = typed.types().builtin(BuiltinType::Int);
    if matches!(shape, Shape::Function) {
        return matches!(typed.types().get(ty), Some(TypeKind::Function { move_only: false, parameters, return_type })
            if parameters.is_empty() && Some(*return_type) == typed.types().builtin(BuiltinType::Unit));
    }
    let inner = match shape {
        Shape::Map => ty,
        _ => match typed.types().get(ty) {
            Some(TypeKind::Nullable(inner)) => *inner,
            _ => return false,
        },
    };
    let Some(TypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::MutableMap,
        arguments,
    }) = typed.types().get(inner)
    else {
        return false;
    };
    let [key, value] = arguments.as_slice() else {
        return false;
    };
    Some(*key) == int
        && if matches!(shape, Shape::NullableMapOfNullableInt) {
            matches!(typed.types().get(*value), Some(TypeKind::Nullable(value)) if Some(*value) == int)
        } else {
            Some(*value) == int
        }
}

fn unit_shape(typed: &CompilationUnitTypes, ty: UnitTypeId, shape: Shape) -> bool {
    let int = typed.types().builtin(BuiltinType::Int);
    if matches!(shape, Shape::Function) {
        return matches!(typed.types().get(ty), Some(UnitTypeKind::Function { move_only: false, parameters, return_type })
            if parameters.is_empty() && Some(*return_type) == typed.types().builtin(BuiltinType::Unit));
    }
    let inner = match shape {
        Shape::Map => ty,
        _ => match typed.types().get(ty) {
            Some(UnitTypeKind::Nullable(inner)) => *inner,
            _ => return false,
        },
    };
    let Some(UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::MutableMap,
        arguments,
    }) = typed.types().get(inner)
    else {
        return false;
    };
    let [key, value] = arguments.as_slice() else {
        return false;
    };
    Some(*key) == int
        && if matches!(shape, Shape::NullableMapOfNullableInt) {
            matches!(typed.types().get(*value), Some(UnitTypeKind::Nullable(value)) if Some(*value) == int)
        } else {
            Some(*value) == int
        }
}

pub(super) fn single(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    trace: &mut Trace,
) {
    let locals = syntax::inspect(case, sources, parsed, names, false, trace);
    let counts = [
        typed.map_constructions().len(),
        typed.map_sizes().len(),
        typed.map_contains_calls().len(),
        typed.map_gets().len(),
        typed.map_puts().len(),
        typed.map_removes().len(),
        typed.map_with_values().len(),
        typed.map_consumes().len(),
        typed.map_require_values().len(),
        typed.non_null_assertions().len(),
    ];
    trace.check(
        "formation exact descriptor counts, no result pre-registration",
        counts == [case.constructions, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        counts,
    );
    for local in &locals {
        let storage = typed.symbol_type(local.symbol);
        let actual = typed.expression_type(local.initializer);
        let declared = local.type_ref.and_then(|id| typed.type_ref_type(id));
        trace.check(
            "exact local symbol storage and declared type",
            storage.is_some_and(|ty| single_shape(typed, ty, local.expected.shape))
                && (local.type_ref.is_none() || declared == storage),
            (local.item, local, storage, declared, actual),
        );
        let valid = match local.expected.initializer {
            Initializer::EmptyMap => actual.is_some_and(|ty| single_shape(typed, ty, Shape::Map))
                && typed.map_construction(local.initializer).is_some_and(|descriptor| {
                    descriptor.expression() == local.initializer
                        && descriptor.callable() == IntrinsicCallable::MutableMapOf
                        && Some(descriptor.map_type()) == actual
                        && Some(descriptor.key_type()) == typed.types().builtin(BuiltinType::Int)
                        && Some(descriptor.value_type()) == typed.types().builtin(BuiltinType::Int)
                }),
            Initializer::Null | Initializer::Lambda => actual.is_some() && actual == storage,
            Initializer::Return => actual == typed.types().builtin(BuiltinType::Nothing),
            Initializer::Name(index) => locals.get(index).is_some_and(|previous| {
                let span = parsed.ast().expressions().get(local.initializer).ok().map(|node| node.span());
                let references: Vec<_> = names.references().iter().filter(|reference| Some(reference.span()) == span).collect();
                trace.stages.push(format!("actual move source local={local:#?}; references={references:#?}"));
                actual.is_some() && actual == typed.symbol_type(previous.symbol)
                    && matches!(references.as_slice(), [reference] if matches!(reference.target(), ReferenceTarget::Symbol(symbol) if *symbol == previous.symbol))
            }),
        };
        trace.check(
            "initializer has exact typed payload and source identity",
            valid,
            (local, actual, typed.map_construction(local.initializer)),
        );
    }
    trace.stages.push(format!(
        "actual formation-local symbols={locals:#?}; map constructions={:#?}",
        typed.map_constructions()
    ));
    // The shared ordinary pipeline records complete Drop facts after ownership.
    // P2 deliberately does not assume any missing Drop proves formation.
}

pub(super) fn unit(
    case: &Case,
    sources: &SourceMap,
    file: (SourceUnitId, &ParsedFile),
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    trace: &mut Trace,
) {
    let resolutions: Vec<_> = names
        .names()
        .source_units()
        .iter()
        .filter(|source| {
            source.source_unit() == file.0 && source.resolution().source_id() == file.1.source_id()
        })
        .collect();
    trace.check(
        "one exact provider source-qualified name resolution",
        resolutions.len() == 1,
        &resolutions,
    );
    let [resolution] = resolutions.as_slice() else {
        return;
    };
    let locals = syntax::inspect(case, sources, file.1, resolution.resolution(), true, trace);
    let symbols: Vec<Option<UnitSymbolId>> = locals
        .iter()
        .map(|local| {
            let candidates: Vec<_> = typed
                .body_symbol_types()
                .keys()
                .copied()
                .filter(|symbol| symbol.source_unit() == file.0 && symbol.symbol() == local.symbol)
                .collect();
            trace.check(
                "same source-qualified body symbol identity",
                candidates.len() == 1,
                (&candidates, local),
            );
            match candidates.as_slice() {
                [symbol] => Some(*symbol),
                _ => None,
            }
        })
        .collect();
    let counts = [
        typed.map_constructions().len(),
        typed.map_sizes().len(),
        typed.map_contains_calls().len(),
        typed.map_gets().len(),
        typed.map_puts().len(),
        typed.map_removes().len(),
        typed.map_with_values().len(),
        typed.map_consumes().len(),
        typed.map_require_values().len(),
        typed.non_null_assertions().len(),
    ];
    trace.check(
        "formation exact qualified descriptor counts, no result pre-registration",
        counts == [case.constructions, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        counts,
    );
    for (local, symbol) in locals.iter().zip(&symbols) {
        let expression = UnitExpressionId::new(file.0, local.initializer);
        let storage = symbol.and_then(|symbol| typed.symbol_type(symbol));
        let actual = typed.expression_type(expression);
        let declared = local
            .type_ref
            .and_then(|id| typed.type_ref_type(UnitTypeRefId::new(file.0, id)));
        trace.check(
            "exact qualified local storage and declared type",
            storage.is_some_and(|ty| unit_shape(typed, ty, local.expected.shape))
                && (local.type_ref.is_none() || declared == storage),
            (local.item, local, symbol, storage, declared, actual),
        );
        let valid = match local.expected.initializer {
            Initializer::EmptyMap => actual.is_some_and(|ty| unit_shape(typed, ty, Shape::Map))
                && typed.map_construction(expression).is_some_and(|descriptor| {
                    descriptor.expression() == expression
                        && descriptor.callable() == IntrinsicCallable::MutableMapOf
                        && Some(descriptor.map_type()) == actual
                        && Some(descriptor.key_type()) == typed.types().builtin(BuiltinType::Int)
                        && Some(descriptor.value_type()) == typed.types().builtin(BuiltinType::Int)
                }),
            Initializer::Null | Initializer::Lambda => actual.is_some() && actual == storage,
            Initializer::Return => actual == typed.types().builtin(BuiltinType::Nothing),
            Initializer::Name(index) => symbols.get(index).copied().flatten().is_some_and(|previous| {
                let span = file.1.ast().expressions().get(local.initializer).ok().map(|node| node.span());
                let references: Vec<_> = names.names().references().iter().filter(|reference|
                    reference.source_unit() == file.0 && Some(reference.span()) == span).collect();
                trace.stages.push(format!("actual qualified move source={symbol:?}; local={local:#?}; references={references:#?}"));
                actual.is_some() && actual == typed.symbol_type(previous)
                    && matches!(references.as_slice(), [reference] if matches!(reference.target(), UnitReferenceTarget::Symbol(symbol) if *symbol == previous))
            }),
        };
        trace.check(
            "initializer has exact qualified payload and source identity",
            valid,
            (local, symbol, actual, typed.map_construction(expression)),
        );
    }
    trace.stages.push(format!(
        "actual qualified formation locals={locals:#?}; symbols={symbols:#?}; constructions={:#?}",
        typed.map_constructions()
    ));
}
