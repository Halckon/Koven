//! Resource local future-use corners: lexical cleanup after all later reads and markers.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod facts {
    //! Original Resource formation and live cross-For owners; never infer Pure ASAP cleanup.
    use super::super::local_future::facts::ast;
    mod borrows {
        //! Real future caller loans end normally while the Resource owner remains live.
        use super::*;
        use lang_frontend::{
            parser::{Item, NameMarker},
            type_checking::UnitTypeId,
        };
        fn reader(sources: &SourceMap, parsed: &ParsedFile) -> Result<(Span, Span), Failure> {
            one(
                parsed.roots().iter().filter_map(|id| {
                    let n = parsed.ast().items().get(*id).ok()?;
                    match n.payload() {
                        Item::Function {
                            name: NameMarker::Present(name),
                            parameters,
                            ..
                        } if sources.slice(*name) == Ok("readMap") && parameters.len() == 1 => {
                            Some((*name, parameters[0].span))
                        }
                        _ => None,
                    }
                }),
                "Resource future original readMap root/parameter",
            )
        }

        pub(super) fn unit(
            input: (&Case, &SourceMap, &ParsedFile, &Scanner),
            names: &ValidatedCompilationUnitNames,
            typed: &CompilationUnitTypes,
            owned: &CompilationUnitOwnership,
            source: (UnitSymbolId, UnitTypeId),
        ) -> Result<(), Failure> {
            let (case, sources, parsed, site) = input;
            let (root, ty) = source;
            let unit = root.source_unit();
            let (name, parameter) = reader(sources, parsed)?;
            let target = one(
                typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter(|d| d.callable().is_some_and(|c| c.name_span() == name)),
                "Resource future qualified reader declaration",
            )?;
            let calls = typed
                .calls()
                .iter()
                .filter(|c| {
                    c.target() == UnitCallTarget::Declaration(target.declaration())
                        && ast::span(parsed, c.expression().expression())
                            .is_ok_and(|s| ast::within(site.span, s))
                })
                .collect::<Vec<_>>();
            let sizes = typed
                .map_sizes()
                .iter()
                .filter(|s| {
                    s.expression().source_unit() == unit
                        && ast::span(parsed, s.expression().expression())
                            .is_ok_and(|s| ast::within(site.span, s))
                })
                .collect::<Vec<_>>();
            need(
                calls.len() == if case.form == 0 { 2 } else { 1 }
                    && sizes.len() == usize::from(case.form == 0),
                "Resource future qualified actual read set",
                (&calls, &sizes),
            )?;
            let mut uses = Vec::new();
            for call in calls {
                let arg =
                    UnitExpressionId::new(unit, ast::argument(parsed, call.expression().expression())?);
                let contracts = owned
                    .call_argument_contracts_for(call.expression())
                    .collect::<Vec<_>>();
                need(
                    qualified(parsed, names, arg.expression())? == root
                        && matches!(call.arguments(),[a] if a.mode()==ParameterMode::Borrow && a.argument_index()==0 && a.parameter_index()==0 && a.parameter_type()==ty)
                        && matches!(contracts.as_slice(),[c] if c.argument()==arg && c.parameter_index()==0 && c.kind()==UnitCallArgumentOwnershipKind::SharedLoan)
                        && !owned
                            .value_deliveries()
                            .iter()
                            .any(|d| d.call() == call.expression()),
                    "Resource future qualified true Shared caller contract without Move",
                    (call, contracts),
                )?;
                uses.push((call.expression(), arg, Some(parameter)));
            }
            for size in sizes {
                need(
                    size.map_type() == ty
                        && qualified(parsed, names, size.receiver().expression())? == root,
                    "Resource future qualified true size",
                    size,
                )?;
                uses.push((size.expression(), size.receiver(), None));
            }
            let loans = owned
                .loans()
                .iter()
                .filter(|l| matches!(l.target(),UnitLoanTarget::Place(p) if p.is_root() && p.root()==root))
                .collect::<Vec<_>>();
            need(
                loans.len() == uses.len(),
                "Resource future qualified complete actual loan set",
                &loans,
            )?;
            for (call, arg, parameter) in uses {
                let loan = one(
                    loans
                        .iter()
                        .copied()
                        .filter(|l| l.call() == call && l.argument() == arg),
                    "Resource future qualified actual loan",
                )?;
                need(
                    loan.kind() == LoanKind::Shared
                        && loan.begin_span() == ast::span(parsed, arg.expression())?
                        && loan.end_span() == ast::span(parsed, call.expression())?
                        && loan.parameter_span() == parameter,
                    "Resource future qualified original CallReturn span/parameter",
                    loan,
                )?;
            }
            Ok(())
        }
    }
    use super::super::local_resources::facts::nominal;
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::{Case, Report};
    use ast::Scanner;
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::{
            SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
        },
        ownership_checking::{
            CompilationUnitOwnership, DropPoint, IterationExitKind as E, IterationSource, LoanKind,
            MapIterationComponent, OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropPoint,
            UnitDropTarget, UnitIterationCleanupAction as UA, UnitIterationExitKind as UE,
            UnitIterationSourceAccess, UnitLoanTarget,
        },
        parser::{Expression, ParsedFile},
        source::{SourceMap, Span},
        type_checking::{
            BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
            IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
            UnitExpressionId, UnitStatementId, UnitTypeKind,
        },
    };

    fn qualified(
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        id: ExpressionId,
    ) -> Result<UnitSymbolId, Failure> {
        let span = ast::span(parsed, id)?;
        one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == span)
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                }),
            "Resource future original qualified Name root",
        )
    }
    fn marker(
        names: &ValidatedCompilationUnitNames,
        id: UnitSymbolId,
        span: Span,
        kind: SymbolKind,
    ) -> bool {
        names.names().source_units().iter().any(|u| {
            u.source_unit() == id.source_unit()
                && u.resolution().symbols().iter().any(|s| {
                    s.id() == id.symbol() && s.span() == span && s.kind() == kind && !s.is_synthetic()
                })
        })
    }
    fn lexical(site: &Scanner, parsed: &ParsedFile) -> Result<ExpressionId, Failure> {
        one(
            parsed.ast().expressions().iter().filter_map(|(id, n)| {
                (ast::within(site.span, n.span()) && matches!(n.payload(), Expression::Return { .. }))
                    .then_some(id)
            }),
            "Resource future real scanner lexical Return",
        )
    }

    pub(super) fn unit(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        report: &mut Report,
    ) {
        match ast::scanners((case.binding, case.exit, case.form), sources, parsed) {
            Ok(sites) => {
                for site in &sites {
                    report.query(
                        "Resource future original qualified formation/cleanup/loans",
                        &unit_site((case, sources, parsed), site, names, typed, owned),
                    );
                }
            }
            Err(e) => report.query::<()>("Resource future actual AST", &Err(e)),
        }
    }
    fn unit_site(
        input: (&Case, &SourceMap, &ParsedFile),
        site: &Scanner,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        let (case, sources, parsed) = input;
        let root = qualified(parsed, names, site.loops[0].source)?;
        let unit = root.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        let original = if site.root.initializer == site.birth.initializer {
            root
        } else {
            qualified(parsed, names, site.root.initializer)?
        };
        for (id, local) in [(root, &site.root), (original, &site.birth)] {
            need(
                marker(names, id, local.marker, SymbolKind::Variable)
                    && id.source_unit() == unit
                    && owned.formed_owned_locals().contains(&id),
                "Resource future qualified actual formation history",
                (id, local),
            )?;
        }
        need(
            (root == original) == (case.binding != 2)
                && (root == original
                    || !owned
                        .drops()
                        .iter()
                        .any(|d| d.target() == UnitDropTarget::Named(original)))
                && !owned.drops().iter().any(|d| {
                    d.target() == UnitDropTarget::Temporary(expression(site.birth.initializer))
                }),
            "Resource future qualified Move obligation remains unique",
            (root, original),
        )?;
        let ty = typed
            .symbol_type(root)
            .ok_or_else(|| failure("Resource future qualified source type", root))?;
        let constructor = if case.mutable {
            IntrinsicTypeConstructor::MutableMap
        } else {
            IntrinsicTypeConstructor::Map
        };
        need(
            typed.symbol_type(original) == Some(ty)
                && typed.is_resource_type(ty) == Some(true)
                && typed.expression_type(expression(site.birth.initializer)) == Some(ty)
                && typed.expression_category(expression(site.birth.initializer))
                    == Some(ExpressionCategory::Temporary)
                && typed.expression_type(expression(site.root.initializer)) == Some(ty)
                && typed.expression_category(expression(site.root.initializer))
                    == Some(if root == original {
                        ExpressionCategory::Temporary
                    } else {
                        ExpressionCategory::Place
                    })
                && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && arguments.len()==2 && typed.types().get(arguments[0])==Some(&UnitTypeKind::Builtin(BuiltinType::String))),
            "Resource future qualified exact Resource birth/type/category",
            ty,
        )?;
        let factory = one(
            typed.signatures().declarations().iter().filter(|d| {
                d.callable()
                    .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
            }),
            "Resource future qualified original factory",
        )?;
        let call = typed
            .call(expression(site.birth.initializer))
            .ok_or_else(|| failure("Resource future qualified actual Call", site))?;
        need(
            call.target() == UnitCallTarget::Declaration(factory.declaration())
                && call.arguments().is_empty()
                && call.receiver().is_none()
                && call.return_type() == ty
                && call.result_source() == CallableResultSource::Owned,
            "Resource future qualified selected factory Call",
            call,
        )?;
        for loop_site in &site.loops {
            let statement = UnitStatementId::new(unit, loop_site.statement);
            let p = owned
                .map_iteration(statement)
                .ok_or_else(|| failure("Resource future original qualified For plan", loop_site))?;
            let d = p.descriptor();
            need(
                typed.map_iteration(statement) == Some(d)
                    && d.source() == expression(loop_site.source)
                    && d.source_type() == ty
                    && d.kind()
                        == if case.mutable {
                            MapIterationKind::MutableMap
                        } else {
                            MapIterationKind::Map
                        }
                    && qualified(parsed, names, loop_site.source)? == root
                    && typed.expression_category(expression(loop_site.source))
                        == Some(ExpressionCategory::Place)
                    && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{arguments,..}) if arguments.as_slice()==[d.key_type(),d.value_type()])
                    && p.source_access() == UnitIterationSourceAccess::Owned
                    && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
                "Resource future original qualified For/source/Owned identity",
                p,
            )?;
            nominal::unit(case.nullable, sources, parsed, names, typed, d.value_type())?;
            for (index, binding) in d.bindings().into_iter().enumerate() {
                need(
                    marker(
                        names,
                        binding,
                        loop_site.bindings[index],
                        SymbolKind::ForBinding,
                    ) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                        && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                        && p.bindings()[index].symbol() == binding
                        && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                        && p.loans()[index].statement() == statement
                        && p.loans()[index].source() == d.source()
                        && p.loans()[index].binding() == binding
                        && p.loans()[index].component()
                            == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                        && p.loans()[index].kind() == LoanKind::Shared
                        && Some(p.loans()[index].origin()) == p.source(),
                    "Resource future qualified nullable/direct original K/V",
                    p.loans(),
                )?;
            }
            let point = match loop_site.point {
                DropPoint::AfterStatement(s) => {
                    UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
                }
                DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
                other => return Err(failure("Resource future qualified actual point", other)),
            };
            let kind = match loop_site.kind {
                E::Fallthrough => UE::Fallthrough,
                E::Continue(e) => UE::Continue(expression(e)),
                E::Break(e) => UE::Break(expression(e)),
                other => return Err(failure("Resource future qualified actual kind", other)),
            };
            let guard = *one(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.value_origin() == loop_site.guard.marker),
                "Resource future qualified actual Guard Drop",
            )?;
            need(
                guard.point() == point
                    && matches!(guard.target(),UnitDropTarget::Named(id) if marker(names,id,loop_site.guard.marker,SymbolKind::Variable) && owned.formed_owned_locals().contains(&id)),
                "Resource future qualified original Guard point/target",
                guard,
            )?;
            need(
                p.exits().len() == 2,
                "Resource future qualified full exit set",
                p.exits(),
            )?;
            for (kind, point, body) in [
                (kind, point, true),
                (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
            ] {
                let exit = one(
                    p.exits()
                        .iter()
                        .filter(|e| e.kind() == kind && e.point() == point),
                    "Resource future qualified actual exit",
                )?;
                let mut expected = if body {
                    vec![
                        UA::Drop(guard),
                        UA::EndBinding {
                            statement,
                            symbol: d.value_binding(),
                        },
                        UA::EndBinding {
                            statement,
                            symbol: d.key_binding(),
                        },
                    ]
                } else {
                    vec![]
                };
                if !body || matches!(kind, UE::Break(_)) {
                    expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
                }
                need(
                    exit.actions() == expected
                        && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
                    "Resource future qualified complete For cleanup keeps source",
                    (exit, expected),
                )?;
            }
        }
        let drop = *one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(root)),
            "Resource future qualified unique lexical Drop",
        )?;
        need(
            drop.point() == UnitDropPoint::ControlTransfer(expression(lexical(site, parsed)?))
                && drop.value_origin() == site.root.marker,
            "Resource future qualified true lexical Drop",
            drop,
        )?;
        borrows::unit(
            (case, sources, parsed, site),
            names,
            typed,
            owned,
            (root, ty),
        )
    }
}
mod graph;
mod sources {
    //! Twenty-four cross-axis corners; formation is assigned, never advertised as a product axis.
    pub(crate) struct Case {
        pub label: String,
        pub source: String,
        pub golden: String,
        pub mutable: bool,
        pub nullable: bool,
        pub binding: usize,
        pub exit: usize,
        pub form: usize,
    }
    pub(crate) fn sources() -> Vec<Case> {
        let mut cases = Vec::new();
        for mutable in [false, true] {
            for nullable in [false, true] {
                for exit in 0..3 {
                    for continuation in 0..2 {
                        cases.push(source(mutable, nullable, exit, continuation));
                    }
                }
            }
        }
        assert_eq!(cases.len(), 24);
        assert_eq!(
            cases
                .iter()
                .map(|c| &c.label)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            24
        );
        for binding in 0..3 {
            assert_eq!(cases.iter().filter(|c| c.binding == binding).count(), 8);
        }
        for mutable in [false, true] {
            for nullable in [false, true] {
                for exit in 0..3 {
                    for form in [0, 2] {
                        assert_eq!(
                            cases
                                .iter()
                                .filter(|c| c.mutable == mutable
                                    && c.nullable == nullable
                                    && c.exit == exit
                                    && c.form == form)
                                .count(),
                            1
                        );
                    }
                }
            }
        }
        cases
    }
    fn source(mutable: bool, nullable: bool, exit: usize, continuation: usize) -> Case {
        let binding = (usize::from(mutable) + usize::from(nullable) + exit + continuation) % 3;
        let form = if continuation == 0 { 0 } else { 2 };
        let kind = if mutable { "MutableMap" } else { "Map" };
        let payload = if nullable { "Resource?" } else { "Resource" };
        let factory = if mutable { "mutableMapOf" } else { "mapOf" };
        let conversion = if mutable {
            "return source"
        } else {
            "val result=source.consume();return result"
        };
        let middle = if nullable {
            "source.put(\"n\".clone(),null);"
        } else {
            ""
        };
        let read_value = if nullable {
            "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}"
        } else {
            "println(\"value\")"
        };
        let transfer = ["", "continue", "break"][exit];
        let after = if form == 0 {
            "readMap(source);println(\"between\");readMap(source);val count=source.size;println(\"after-last-use\");if(count==0){println(\"zero\")}else{println(\"nonzero\")};println(\"tail\");return 0"
        } else {
            "println(\"second\");for((key,value) in source){val local=Guard();read(key,value)};readMap(source);println(\"after-last-use\");println(\"tail\");return 0"
        };
        let mut scans = String::new();
        for empty in [false, true] {
            let scanner = if empty { "scanEmpty" } else { "scan" };
            let maker = if empty { "makeEmpty" } else { "make" };
            let declaration = match binding {
                0 => format!("val source={maker}()"),
                1 => format!("var source={maker}()"),
                _ => format!("val original={maker}();val source=original"),
            };
            scans.push_str(&format!("fun {scanner}():Int{{{declaration};println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};{after}}}\n"));
        }
        let source = format!(
            r#"class Resource(val tag:String){{deinit(){{println(this.tag)}}}}
    class Guard{{deinit(){{println("guard-drop")}}}}
    fun read(key:String,value:{payload}):Unit{{println(key);{read_value}}}
    fun readMap(source:{kind}<String,{payload}>):Unit{{if(source.size==0){{println("borrow-zero")}}else{{println("borrow-nonzero")}}}}
    fun make():{kind}<String,{payload}>{{var source=mutableMapOf<String,{payload}>();source.put("z".clone(),Resource("resource-drop".clone()));{middle}source.put("a".clone(),Resource("resource-drop".clone()));{conversion}}}
    fun makeEmpty():{kind}<String,{payload}>{{return {factory}<String,{payload}>()}}
    {scans}
    fun entry():Unit{{println("nonempty");val first=scan();if(first==0){{println("done")}}else{{error("bad return")}};println("empty");val second=scanEmpty();if(second==0){{println("done")}}else{{error("bad empty return")}};println("entry-end")}}
    "#
        );
        let entries: &[(&str, &str)] = if nullable {
            &[("z", "some"), ("n", "none"), ("a", "some")]
        } else {
            &[("z", "value"), ("a", "value")]
        };
        let mut golden = String::new();
        for empty in [false, true] {
            golden.push_str(if empty {
                "empty\nformed\n"
            } else {
                "nonempty\nformed\n"
            });
            if !empty {
                for (key, value) in entries
                    .iter()
                    .take(if exit == 2 { 1 } else { entries.len() })
                {
                    golden.push_str(&format!("{key}\n{value}\nguard-drop\n"));
                }
            }
            let borrowed = if empty {
                "borrow-zero\n"
            } else {
                "borrow-nonzero\n"
            };
            if form == 0 {
                golden.push_str(borrowed);
                golden.push_str("between\n");
                golden.push_str(borrowed);
                golden.push_str("after-last-use\n");
                golden.push_str(if empty { "zero\n" } else { "nonzero\n" });
            } else {
                golden.push_str("second\n");
                if !empty {
                    for (key, value) in entries {
                        golden.push_str(&format!("{key}\n{value}\nguard-drop\n"));
                    }
                }
                golden.push_str(borrowed);
                golden.push_str("after-last-use\n");
            }
            golden.push_str("tail\n");
            if !empty {
                golden.push_str("resource-drop\nresource-drop\n");
            }
            golden.push_str("done\n");
        }
        golden.push_str("entry-end\n");
        Case {
            label: format!(
                "local-resource-future-{kind}-{}-{}-{}-{}",
                if nullable { "nullable" } else { "direct" },
                ["normal", "continue", "break"][exit],
                if form == 0 {
                    "calls-size"
                } else {
                    "repeat-borrow"
                },
                ["val", "var", "alias"][binding]
            ),
            source,
            golden,
            mutable,
            nullable,
            binding,
            exit,
            form,
        }
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use sources::{Case, sources};

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "RESOURCE LOCAL FUTURE INPUT case={} unit={unit} source={:?} golden={:?}",
        case.label,
        case.source,
        case.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let result = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "Resource future actual qualified plan count",
                    typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 }
                        && owned.map_iterations().len() == typed.map_iterations().len(),
                    typed.map_iterations(),
                );
                diagnostic!(
                    "RESOURCE LOCAL FUTURE UNIT case={} formed={:#?} plans={:#?} drops={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.loans()
                );
            },
        )
    } else {
        pipeline::memory_document_observed(
            &sources,
            &parsed,
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                facts::unit(case, &sources, &parsed, names, typed, owned, report);
                report.check(
                    "Resource future actual qualified plan count",
                    typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 }
                        && owned.map_iterations().len() == typed.map_iterations().len(),
                    typed.map_iterations(),
                );
                diagnostic!(
                    "RESOURCE LOCAL FUTURE UNIT case={} formed={:#?} plans={:#?} drops={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.loans()
                );
            },
        )
    };
    diagnostic!(
        "RESOURCE LOCAL FUTURE REPORT case={} unit={unit} report={report:#?}",
        case.label
    );
    let (program, _) = result?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    if !report.passed() {
        return Err(failure("original Resource future FE", report));
    }
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    graph::check(case, &sources, &parsed, &program)?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}
#[test]
fn resource_local_future_uses_preserve_lexical_owners() {
    let mut results = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            diagnostic!(
                "RESOURCE LOCAL FUTURE CASE {} unit={unit} result={result:?}",
                case.label
            );
            results.push((case.label.clone(), unit, result));
        }
    }
    assert_diagnostic!(
        results.len() == 48 && results.iter().all(|(_, _, r)| r.is_ok()),
        "{results:#?}"
    );
}
