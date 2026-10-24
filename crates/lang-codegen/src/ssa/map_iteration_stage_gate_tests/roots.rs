//! 同一 sealed view 逐真实声明 lower；结果逐根验证，不拼接 Program 或重跑 frontend。
use super::{
    Report,
    fixture::{Fixture, LowerOutcome, Stopped, stopped},
};
use crate::ssa::{
    LoweringError,
    model::{FunctionId, Origin, Program},
};
use lang_frontend::{
    name_resolution::{DeclarationId, SymbolKind, UnitDeclaration, ValidatedCompilationUnitNames},
    parser::{Item, NameMarker},
    source::Span,
    type_checking::{CompilationUnitTypes, UnitCallTarget},
};

pub(super) fn collect(
    fixture: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    selected: DeclarationId,
    report: &mut Report,
    mut lower: impl FnMut(DeclarationId, &mut Report) -> Result<(Program, FunctionId), LoweringError>,
) -> Result<LowerOutcome, Stopped> {
    let declarations = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| declaration.kind() == SymbolKind::Function)
        .collect::<Vec<_>>();
    let source_roots = fixture
        .files
        .iter()
        .flat_map(|file| {
            file.parsed.roots().iter().map(|root| {
                file.parsed.ast().items().get(*root).ok().and_then(|node| {
                    let Item::Function {
                        name: NameMarker::Present(name),
                        ..
                    } = node.payload()
                    else {
                        return None;
                    };
                    Some((*root, *name, node.span()))
                })
            })
        })
        .collect::<Vec<_>>();
    report.check(
        "unit every original function root has one real declaration",
        !source_roots.is_empty()
            && source_roots.len() == declarations.len()
            && source_roots.iter().all(|root| {
                root.is_some_and(|(item, name, _)| {
                    declarations
                        .iter()
                        .filter(|declaration| {
                            declaration.root() == item && declaration.name_span() == name
                        })
                        .count()
                        == 1
                })
            })
            && declarations
                .iter()
                .filter(|declaration| declaration.id() == selected)
                .count()
                == 1,
        (&source_roots, &declarations, selected),
    );
    let mut selected_outcome = None;
    for declaration in &declarations {
        let expected = fixture.function_location(declaration.name_span());
        check_dependencies(fixture, names, typed, declaration, report);
        // query 失败不能取消已获准的 lower；每根错误也不能遮住其余根。
        let lowered = lower(declaration.id(), report);
        report.check(
            "unit real declaration lower actually attempted",
            true,
            (declaration.id(), declaration.name(), lowered.as_ref().err()),
        );
        if let Ok((program, entry)) = &lowered {
            let actual_sources = program
                .modules
                .iter()
                .flat_map(|module| &module.functions)
                .filter_map(|function| {
                    declarations.iter().find_map(|candidate| {
                        fixture
                            .function_location(candidate.name_span())
                            .filter(|location| function.origin == Origin::Source(location.span))
                            .map(|location| (candidate.id(), function.id(), location))
                    })
                })
                .collect::<Vec<_>>();
            // 这些原 fixture 只调用 builtin println，各根没有源码 callable 依赖。
            // 每个 Program 必须包含自己的根，不能借别的根 Program 补漏。
            report.check(
                "unit per-root actual source dependency coverage",
                matches!(actual_sources.as_slice(), [(root, function, location)]
                    if *root == declaration.id() && *function == *entry
                        && Some(location) == expected.as_ref()),
                (declaration.id(), &actual_sources),
            );
        }
        let outcome = LowerOutcome::new(&fixture.sources, expected, lowered);
        report.check(
            "unit per-root source identity SSA and complete LLVM",
            outcome.matches(),
            (declaration.id(), declaration.name(), &outcome),
        );
        if declaration.id() == selected {
            selected_outcome = Some(outcome);
        }
    }
    selected_outcome.ok_or_else(|| stopped("unit selected root outcome", selected))
}

fn check_dependencies(
    fixture: &Fixture,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    declaration: &UnitDeclaration,
    report: &mut Report,
) {
    let source = names
        .names()
        .index()
        .source_units()
        .iter()
        .find(|source| source.id() == declaration.source_unit());
    let file = source.and_then(|source| {
        fixture
            .files
            .iter()
            .find(|file| file.parsed.source_id() == source.source_id())
    });
    let expected = fixture.function_location(declaration.name_span());
    report.check(
        "unit root retains original source-qualified AST identity",
        file.is_some_and(|file| {
            file.parsed.roots().contains(&declaration.root())
                && file.parsed.ast().items().get(declaration.root()).is_ok_and(|node| {
                    matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                        if *name == declaration.name_span())
                        && expected.as_ref().is_some_and(|expected| expected.span == node.span())
                })
        }),
        (declaration, &expected),
    );
    let mut calls = Vec::new();
    let mut complete = true;
    for call in typed
        .calls()
        .iter()
        .filter(|call| call.expression().source_unit() == declaration.source_unit())
    {
        let span = file.and_then(|file| {
            file.parsed
                .ast()
                .expressions()
                .get(call.expression().expression())
                .ok()
                .map(|node| node.span())
        });
        match (span, expected.as_ref()) {
            (Some(span), Some(expected)) if within(span, expected.span) => calls.push(call),
            (Some(_), Some(_)) => {}
            _ => complete = false,
        }
    }
    report.check(
        "unit original root has only actual builtin println dependencies",
        complete
            && calls.iter().all(|call| {
                matches!(call.target(), UnitCallTarget::External(_)) && call.prints_line()
            }),
        (declaration.id(), calls),
    );
}

fn within(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
