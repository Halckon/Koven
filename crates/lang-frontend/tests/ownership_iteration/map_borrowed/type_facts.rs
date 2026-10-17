//! 当前公开类型/name 查询：intrinsic K/V、真实两 binding/读取及同步调用参数。
use super::{
    Report,
    cases::Case,
    fixture::{Fixture, Sites},
};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, SourceUnitId, Symbol, SymbolId, SymbolKind,
        UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    source::Span,
    type_checking::{
        BuiltinType, CallableTarget, CompilationUnitTypes, ParameterMode, TypeKind, TypedFile,
        UnitCallTarget, UnitExpressionId, UnitTypeKind,
    },
};

pub(super) fn symbol_at(names: &NameResolution, span: Span) -> Option<&Symbol> {
    let mut found = names
        .symbols()
        .iter()
        .filter(|symbol| symbol.span() == span && !symbol.is_synthetic());
    let result = found.next()?;
    found.next().is_none().then_some(result)
}

fn symbol_named<'a>(names: &'a NameResolution, name: &str) -> Option<&'a Symbol> {
    let mut found = names
        .symbols()
        .iter()
        .filter(|symbol| symbol.name() == name && !symbol.is_synthetic());
    let result = found.next()?;
    found.next().is_none().then_some(result)
}

pub(super) fn single_reference(names: &NameResolution, span: Span) -> Option<SymbolId> {
    let mut found = names.references().iter().filter_map(|reference| {
        if reference.span() == span
            && let ReferenceTarget::Symbol(symbol) = reference.target()
        {
            Some(*symbol)
        } else {
            None
        }
    });
    let result = found.next()?;
    found.next().is_none().then_some(result)
}

pub(super) fn unit_reference(
    names: &ValidatedCompilationUnitNames,
    span: Span,
) -> Option<UnitSymbolId> {
    let mut found = names.names().references().iter().filter_map(|reference| {
        if reference.span() == span
            && let UnitReferenceTarget::Symbol(symbol) = reference.target()
        {
            Some(*symbol)
        } else {
            None
        }
    });
    let result = found.next()?;
    found.next().is_none().then_some(result)
}

/// 从原 body type 表读取已发布的 source-qualified 身份，不构造私有 UnitSymbolId。
pub(super) fn unit_symbol(
    raw: &CompilationUnitTypes,
    unit: SourceUnitId,
    symbol: SymbolId,
) -> Option<UnitSymbolId> {
    raw.body_symbol_types()
        .keys()
        .find(|candidate| candidate.source_unit() == unit && candidate.symbol() == symbol)
        .copied()
}

pub(super) fn single(
    case: &Case,
    fixture: &Fixture,
    sites: &Sites,
    names: &NameResolution,
    raw: &TypedFile,
    report: &mut Report,
) {
    let source = raw
        .expression_type(sites.source)
        .and_then(|ty| raw.types().get(ty));
    let source_matches = matches!(source,
        Some(TypeKind::Intrinsic { constructor, arguments })
        if *constructor == case.constructor && arguments.len() == 2
            && arguments.iter().all(|ty| raw.types().get(*ty) == Some(&TypeKind::Builtin(case.component))));
    report.record("source intrinsic K/V", source_matches, source);
    report.note(
        "existing sequential plan count",
        raw.sequential_iterations().len(),
    );
    if case.source == "source" {
        let target = single_reference(names, sites.source_span);
        let symbol = target.and_then(|id| names.symbols().iter().find(|symbol| symbol.id() == id));
        report.record(
            "named source binding/read identity",
            symbol.is_some_and(|symbol| {
                symbol.name() == "source"
                    && !symbol.is_synthetic()
                    && raw.symbol_type(symbol.id()) == raw.expression_type(sites.source)
            }),
            (target, symbol),
        );
    } else {
        let target = case
            .source
            .strip_suffix("()")
            .and_then(|name| symbol_named(names, name));
        let call = raw.call(sites.source);
        report.record(
            "temporary source actual factory target",
            target.is_some_and(|symbol| {
                call.is_some_and(|call| call.target() == CallableTarget::Source(symbol.id()))
            }),
            (target, call),
        );
    }
    for index in 0..2 {
        let symbol = symbol_at(names, sites.bindings[index]);
        let ty = symbol.and_then(|symbol| raw.symbol_type(symbol.id()));
        report.record(
            "real ForBinding symbol/type",
            symbol.is_some_and(|symbol| symbol.kind() == SymbolKind::ForBinding)
                && ty.and_then(|ty| raw.types().get(ty))
                    == Some(&TypeKind::Builtin(case.component)),
            (index, symbol, ty.and_then(|ty| raw.types().get(ty))),
        );
        for (expression, span) in &sites.reads[index] {
            let target = single_reference(names, *span);
            let read = raw.expression_type(*expression);
            report.record(
                "body read binding/type identity",
                symbol.is_some_and(|symbol| target == Some(symbol.id()))
                    && ty.is_some()
                    && read == ty,
                (index, expression, span, target, read),
            );
        }
    }
    if let Some((call_id, arguments)) = &sites.call {
        let call = raw.call(*call_id);
        let target = case.call.and_then(|name| symbol_named(names, name));
        report.record(
            "ordinary call target",
            target.is_some_and(|symbol| {
                call.is_some_and(|call| call.target() == CallableTarget::Source(symbol.id()))
            }),
            (call.map(|call| call.target()), target),
        );
        let mode = if case.call == Some("read") {
            ParameterMode::Borrow
        } else {
            ParameterMode::Value
        };
        let matches = call.is_some_and(|call| {
            call.expression() == *call_id
                && call.arguments().len() == arguments.len()
                && call.arguments().iter().enumerate().all(|(index, arg)| {
                    arg.argument_index() == index
                        && arg.parameter_index() == index
                        && arg.mode() == mode
                        && raw.types().get(arg.parameter_type())
                            == Some(&TypeKind::Builtin(BuiltinType::String))
                        && raw.expression_type(arguments[index]) == Some(arg.parameter_type())
                })
        });
        report.record("ordinary call argument modes/types", matches, call);
        record_call_operands(case, fixture, sites, arguments, report);
    }
}

pub(super) fn unit(
    case: &Case,
    fixture: &Fixture,
    sites: &Sites,
    names: &ValidatedCompilationUnitNames,
    raw: &CompilationUnitTypes,
    report: &mut Report,
) {
    let [source_names] = names.names().source_units() else {
        report.record(
            "unit source identity",
            false,
            names.names().source_units().len(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let source = raw
        .expression_type(expression(sites.source))
        .and_then(|ty| raw.types().get(ty));
    let source_matches = matches!(source,
        Some(UnitTypeKind::Intrinsic { constructor, arguments })
        if *constructor == case.constructor && arguments.len() == 2
            && arguments.iter().all(|ty| raw.types().get(*ty) == Some(&UnitTypeKind::Builtin(case.component))));
    report.record("source intrinsic K/V", source_matches, source);
    report.note(
        "existing sequential plan count",
        raw.sequential_iterations().len(),
    );
    if case.source == "source" {
        let target = unit_reference(names, sites.source_span);
        let local = target.and_then(|target| {
            source_names
                .resolution()
                .symbols()
                .iter()
                .find(|symbol| target.source_unit() == unit && target.symbol() == symbol.id())
        });
        report.record(
            "named unit source binding/read identity",
            local.is_some_and(|symbol| {
                symbol.name() == "source"
                    && !symbol.is_synthetic()
                    && target.and_then(|symbol| raw.symbol_type(symbol))
                        == raw.expression_type(expression(sites.source))
            }),
            (target, local),
        );
    } else {
        let local = case
            .source
            .strip_suffix("()")
            .and_then(|name| symbol_named(source_names.resolution(), name));
        let call = raw.call(expression(sites.source));
        let target = call.and_then(|call| match call.target() {
            UnitCallTarget::Declaration(declaration) => {
                names.names().declaration_symbol(declaration)
            }
            UnitCallTarget::Symbol(symbol) => Some(symbol),
            _ => None,
        });
        report.record(
            "temporary unit source actual factory target",
            local.zip(target).is_some_and(|(local, target)| {
                target.source_unit() == unit && target.symbol() == local.id()
            }),
            (local, target, call),
        );
    }
    for index in 0..2 {
        let local = symbol_at(source_names.resolution(), sites.bindings[index]);
        let symbol = local.and_then(|symbol| unit_symbol(raw, unit, symbol.id()));
        let ty = symbol.and_then(|symbol| raw.symbol_type(symbol));
        report.record(
            "real unit ForBinding symbol/type",
            local.is_some_and(|symbol| symbol.kind() == SymbolKind::ForBinding)
                && symbol.is_some()
                && ty.and_then(|ty| raw.types().get(ty))
                    == Some(&UnitTypeKind::Builtin(case.component)),
            (index, local, symbol, ty.and_then(|ty| raw.types().get(ty))),
        );
        for (id, span) in &sites.reads[index] {
            let target = unit_reference(names, *span);
            let read = raw.expression_type(expression(*id));
            report.record(
                "unit body read binding/type identity",
                symbol.is_some() && target == symbol && ty.is_some() && read == ty,
                (index, id, span, target, read),
            );
        }
    }
    if let Some((call_id, arguments)) = &sites.call {
        let call = raw.call(expression(*call_id));
        let local = case
            .call
            .and_then(|name| symbol_named(source_names.resolution(), name));
        let selected = call.and_then(|call| match call.target() {
            UnitCallTarget::Declaration(declaration) => {
                names.names().declaration_symbol(declaration)
            }
            UnitCallTarget::Symbol(symbol) => Some(symbol),
            _ => None,
        });
        report.record(
            "ordinary unit call target",
            local.zip(selected).is_some_and(|(local, selected)| {
                selected.source_unit() == unit && selected.symbol() == local.id()
            }),
            (call.map(|call| call.target()), selected, local),
        );
        let mode = if case.call == Some("read") {
            ParameterMode::Borrow
        } else {
            ParameterMode::Value
        };
        let matches = call.is_some_and(|call| {
            call.expression() == expression(*call_id)
                && call.arguments().len() == arguments.len()
                && call.arguments().iter().enumerate().all(|(index, arg)| {
                    arg.argument_index() == index
                        && arg.parameter_index() == index
                        && arg.mode() == mode
                        && raw.types().get(arg.parameter_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                        && raw.expression_type(expression(arguments[index]))
                            == Some(arg.parameter_type())
                })
        });
        report.record("ordinary unit call argument modes/types", matches, call);
        record_call_operands(case, fixture, sites, arguments, report);
    }
}

fn record_call_operands(
    case: &Case,
    fixture: &Fixture,
    sites: &Sites,
    arguments: &[lang_frontend::ast::ExpressionId],
    report: &mut Report,
) {
    // read(key,value) 必须为同一次 call 的两真实 operand；不能只查同名函数签名。
    let expected = sites
        .reads
        .iter()
        .flatten()
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    report.record(
        "call actual binding operands",
        arguments == expected.as_slice(),
        (
            case.call,
            arguments,
            arguments
                .iter()
                .map(|id| {
                    fixture
                        .file
                        .ast()
                        .expressions()
                        .get(*id)
                        .ok()
                        .map(|node| fixture.sources.slice(node.span()))
                })
                .collect::<Vec<_>>(),
        ),
    );
}
