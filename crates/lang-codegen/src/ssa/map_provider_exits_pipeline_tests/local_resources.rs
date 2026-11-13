//! Factory-formed Resource Map locals retain their original owner through all exits.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod boundaries {
    //! Independent live-owner neighbor: two Resource providers reuse the same formed root.
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
    // The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
    use super::*;

    fn sources() -> Vec<Case> {
        let mut cases = super::sources()
            .into_iter()
            .filter(|case| !case.mutable && case.form == 0 && case.exit == 0)
            .map(|mut case| {
                assert_eq!(case.source.matches("};println(\"after\")").count(), 2);
                case.label.push_str("-repeat");
                case.source = case.source.replace(
                    "};println(\"after\")",
                    "};println(\"second\");for((nextKey,nextValue) in source){val nextLocal=Guard();read(nextKey,nextValue)};println(\"after\")",
                );
                let body = if case.nullable {
                    "z\nsome\nguard-drop\nn\nnone\nguard-drop\na\nsome\nguard-drop\n"
                } else {
                    "z\nvalue\nguard-drop\na\nvalue\nguard-drop\n"
                };
                case.golden = case.golden.replacen("after\n", &format!("second\n{body}after\n"), 1)
                    .replace("empty\nformed\nafter\n", "empty\nformed\nsecond\nafter\n");
                case
            })
            .collect::<Vec<_>>();
        cases.sort_by_key(|case| case.nullable);
        assert_eq!(cases.len(), 2);
        cases
    }

    fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
        diagnostic!(
            "G2 RESOURCE REPEAT INPUT case={} unit={unit} source={:?} golden={:?}",
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
                    report.check(
                        "Resource repeat four original qualified providers",
                        typed.map_iterations().len() == 4 && owned.map_iterations().len() == 4,
                        (typed.map_iterations(), owned.map_iterations()),
                    );
                    diagnostic!(
                        "G2 RESOURCE REPEAT UNIT case={} plans={:#?} drops={:#?}",
                        case.label,
                        owned.map_iterations(),
                        owned.drops()
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
                    report.check(
                        "Resource repeat four original qualified providers",
                        typed.map_iterations().len() == 4 && owned.map_iterations().len() == 4,
                        (typed.map_iterations(), owned.map_iterations()),
                    );
                    diagnostic!(
                        "G2 RESOURCE REPEAT UNIT case={} plans={:#?} drops={:#?}",
                        case.label,
                        owned.map_iterations(),
                        owned.drops()
                    );
                },
            )
        };
        diagnostic!(
            "G2 RESOURCE REPEAT REPORT {} unit={unit} report={report:#?}",
            case.label
        );
        let (program, _) = result?;
        report.query(
            "Unit complete temporary source acquisition",
            &source_forms::graph::temporary_entries(&parsed, &program),
        );
        if !report.passed() {
            return Err(failure("original repeat Resource FE", report));
        }
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
        Ok(())
    }

    #[test]
    fn repeated_resource_local_keeps_live_original_owner() {
        let mut outcomes = Vec::new();
        for case in sources() {
            for unit in [false, true] {
                let result = observe(&case, unit);
                diagnostic!(
                    "G2 RESOURCE REPEAT CASE {} unit={unit} result={result:?}",
                    case.label
                );
                outcomes.push((case.label.clone(), unit, result));
            }
        }
        assert_diagnostic!(
            outcomes.len() == 4 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
            "{outcomes:#?}"
        );
    }
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod facts {
    //! Independent public FE formation and lexical Resource cleanup facts.
    use super::super::local_exits::facts::ast;
    pub(in crate::ssa::map_provider_exits_pipeline_tests) mod nominal {
        //! Exact original Resource class, owned String field and nullable component type.
        use super::*;
        use lang_frontend::{
            ast::{ItemId, StatementId},
            parser::{Item, NameMarker},
            type_checking::{NominalKind, UnitItemId, UnitTypeId},
        };
        struct Class {
            name: Span,
            field: Span,
            item: ItemId,
            body: StatementId,
        }
        fn class(sources: &SourceMap, parsed: &ParsedFile) -> Result<Class, Failure> {
            let class = one(
                parsed.roots().iter().filter_map(|id| {
                    let n = parsed.ast().items().get(*id).ok()?;
                    let Item::Classifier(c) = n.payload() else {
                        return None;
                    };
                    matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Resource"))
                        .then_some(c)
                }),
                "original represented Resource class",
            )?;
            let NameMarker::Present(name) = class.name else {
                return Err(failure("original Resource marker", class));
            };
            let field = one(
                class
                    .primary_constructor
                    .as_ref()
                    .ok_or_else(|| failure("actual Resource primary fields", class))?
                    .fields
                    .iter(),
                "actual owned String field",
            )?;
            let NameMarker::Present(field) = field.name else {
                return Err(failure("actual Resource field marker", field));
            };
            need(
                sources.slice(field) == Ok("tag"),
                "actual original tag field",
                field,
            )?;
            let (item, body) = one(
                class
                    .body
                    .as_ref()
                    .ok_or_else(|| failure("actual Resource body", class))?
                    .members
                    .iter()
                    .filter_map(|id| {
                        let n = parsed.ast().items().get(*id).ok()?;
                        match n.payload() {
                            Item::Deinit { body, .. } => Some((*id, *body)),
                            _ => None,
                        }
                    }),
                "actual original Resource deinit",
            )?;
            Ok(Class {
                name,
                field,
                item,
                body,
            })
        }

        pub(in crate::ssa::map_provider_exits_pipeline_tests) fn unit(
            nullable: bool,
            sources: &SourceMap,
            parsed: &ParsedFile,
            names: &ValidatedCompilationUnitNames,
            typed: &CompilationUnitTypes,
            value: UnitTypeId,
        ) -> Result<(), Failure> {
            let class = class(sources, parsed)?;
            let inner = match (nullable, typed.types().get(value)) {
                (true, Some(UnitTypeKind::Nullable(inner))) => *inner,
                (false, Some(UnitTypeKind::Nominal { .. })) => value,
                other => return Err(failure("qualified exact nullable component", other)),
            };
            let Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) = typed.types().get(inner)
            else {
                return Err(failure("actual qualified Resource nominal", inner));
            };
            let nominal = one(
                typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter(|d| d.declaration() == *declaration)
                    .filter_map(|d| d.nominal()),
                "actual qualified Resource signature",
            )?;
            let declared = one(
                names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| d.id() == *declaration && d.name_span() == class.name),
                "original qualified Resource declaration",
            )?;
            let unit = declared.source_unit();
            let field = one(nominal.fields().iter(), "actual qualified Resource field")?;
            let deinit = nominal
                .deinit()
                .ok_or_else(|| failure("actual qualified Resource deinit", nominal))?;
            need(
                arguments.is_empty()
                    && nominal.kind() == NominalKind::Class
                    && field.span() == class.field
                    && typed.types().get(field.ty())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    && deinit.owner() == *declaration
                    && deinit.item() == UnitItemId::new(unit, class.item)
                    && deinit.body() == UnitStatementId::new(unit, class.body)
                    && deinit.receiver_type() == inner
                    && deinit.receiver_mode() == ParameterMode::Borrow,
                "qualified original Resource/String/deinit identity",
                (nominal, deinit),
            )?;
            Ok(())
        }
    }
    mod owner {
        //! Resource owners remain live at Exit until the actual scanner's lexical Return.
        use super::*;

        pub(super) fn lexical(site: &Site, parsed: &ParsedFile) -> Result<DropPoint, Failure> {
            one(
                parsed.ast().expressions().iter().filter_map(|(id, node)| {
                    (matches!(node.payload(), Expression::Return { .. })
                        && within(site.scan, node.span())
                        && node.span().start() > site.span.end())
                    .then_some(DropPoint::ControlTransfer(id))
                }),
                "local Resource actual lexical Return",
            )
        }
    }
    use super::super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::{Case, Report};
    use ast::{Site, expression_span, returned, sites, within};
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::{
            SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
        },
        ownership_checking::{
            CompilationUnitOwnership, DropPoint, IterationExitKind as E, IterationSource, LoanKind,
            MapIterationComponent, OwnershipBindingKind, UnitDropPoint, UnitDropTarget,
            UnitIterationCleanupAction as UA, UnitIterationExitKind as UE,
            UnitIterationSourceAccess, UnitLoanTarget,
        },
        parser::{Expression, ParsedFile},
        source::{SourceMap, Span},
        type_checking::{
            BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
            IntrinsicTypeConstructor, ParameterMode, UnitCallTarget, UnitExpressionId,
            UnitStatementId, UnitTypeKind,
        },
    };

    fn qualified(
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        id: ExpressionId,
    ) -> Result<UnitSymbolId, Failure> {
        let span = expression_span(parsed, id)?;
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
            "Resource qualified Name root",
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
                    s.id() == id.symbol()
                        && s.span() == span
                        && s.kind() == kind
                        && !s.is_synthetic()
                })
        })
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
        match sites((case.mutable, case.form, case.exit), sources, parsed) {
            Ok(sites) => {
                for site in &sites {
                    report.query(
                        "Resource local original qualified identities",
                        &unit_site((case, sources, parsed), site, names, typed, owned),
                    );
                }
            }
            Err(e) => report.query::<()>("Resource local original AST", &Err(e)),
        }
    }
    fn unit_site(
        input: (&Case, &SourceMap, &ParsedFile),
        site: &Site,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        let (case, sources, parsed) = input;
        let root = qualified(parsed, names, site.source)?;
        let original = if site.root.initializer == site.birth.initializer {
            root
        } else {
            qualified(parsed, names, site.root.initializer)?
        };
        let unit = root.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        for (id, local) in [(root, &site.root), (original, &site.birth)] {
            need(
                marker(names, id, local.marker, SymbolKind::Variable)
                    && id.source_unit() == unit
                    && owned.formed_owned_locals().contains(&id),
                "Resource actual qualified formation history",
                (id, local),
            )?;
        }
        need(
            (root == original) == (site.root.marker == site.birth.marker)
                && (root == original
                    || !owned
                        .drops()
                        .iter()
                        .any(|d| d.target() == UnitDropTarget::Named(original)))
                && !owned.drops().iter().any(|d| {
                    d.target() == UnitDropTarget::Temporary(expression(site.birth.initializer))
                }),
            "Resource qualified original Move obligation not duplicated",
            (root, original),
        )?;
        let statement = UnitStatementId::new(unit, site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("Resource actual qualified Map plan", statement))?;
        let d = p.descriptor();
        let ty = d.source_type();
        let constructor = if case.mutable {
            IntrinsicTypeConstructor::MutableMap
        } else {
            IntrinsicTypeConstructor::Map
        };
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == expression(site.source)
                && d.kind() == site.map_kind
                && typed.is_resource_type(ty) == Some(true)
                && typed.is_resource_type(d.value_type()) == Some(true)
                && typed.symbol_type(root) == Some(ty)
                && typed.symbol_type(original) == Some(ty)
                && typed.expression_type(expression(site.source)) == Some(ty)
                && typed.expression_category(expression(site.source))
                    == Some(ExpressionCategory::Place)
                && typed.expression_type(expression(site.root.initializer)) == Some(ty)
                && typed.expression_category(expression(site.root.initializer))
                    == Some(if root == original {
                        ExpressionCategory::Temporary
                    } else {
                        ExpressionCategory::Place
                    })
                && typed.expression_type(expression(site.birth.initializer)) == Some(ty)
                && typed.expression_category(expression(site.birth.initializer))
                    == Some(ExpressionCategory::Temporary)
                && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && arguments.as_slice()==[d.key_type(),d.value_type()])
                && typed.types().get(d.key_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                && p.source_access() == UnitIterationSourceAccess::Owned
                && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
            "Resource qualified original type/category/Owned source",
            p,
        )?;
        nominal::unit(case.nullable, sources, parsed, names, typed, d.value_type())?;
        let factory = one(
            typed.signatures().declarations().iter().filter(|d| {
                d.callable()
                    .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
            }),
            "Resource selected qualified factory",
        )?;
        let call = typed
            .call(expression(site.birth.initializer))
            .ok_or_else(|| failure("Resource original qualified factory Call", site))?;
        need(
            call.target() == UnitCallTarget::Declaration(factory.declaration())
                && call.receiver().is_none()
                && call.arguments().is_empty()
                && call.return_type() == ty
                && call.result_source() == CallableResultSource::Owned,
            "Resource exact qualified factory Call owner",
            call,
        )?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                marker(names, binding, site.bindings[index], SymbolKind::ForBinding)
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
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
                "Resource exact original qualified K/V loan lineage",
                p.loans(),
            )?;
        }
        if let Some((value, args)) = returned(site, parsed)? {
            let call = typed
                .call(expression(value))
                .ok_or_else(|| failure("Resource qualified actual returned Call", value))?;
            need(
                matches!(call.target(),UnitCallTarget::Declaration(id) if typed.signatures().declarations().iter().any(|d|d.declaration()==id && d.callable().is_some_and(|c|c.name_span()==site.result)))
                    && typed.types().get(call.return_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                    && matches!(call.arguments(),[k,v] if k.mode()==ParameterMode::Borrow && v.mode()==ParameterMode::Borrow && k.parameter_type()==d.key_type() && v.parameter_type()==d.value_type())
                    && qualified(parsed, names, args[0])? == d.key_binding()
                    && qualified(parsed, names, args[1])? == d.value_binding(),
                "Resource qualified actual Return keeps original K/V loans",
                call,
            )?;
        }
        let point = match site.point {
            DropPoint::AfterStatement(s) => {
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
            }
            DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
            other => return Err(failure("Resource actual unit body point", other)),
        };
        let lexical = match owner::lexical(site, parsed)? {
            DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
            other => return Err(failure("Resource actual lexical Return", other)),
        };
        let kind = match site.kind {
            E::Fallthrough => UE::Fallthrough,
            E::Continue(e) => UE::Continue(expression(e)),
            E::Break(e) => UE::Break(expression(e)),
            E::Return(e) => UE::Return(expression(e)),
            other => return Err(failure("Resource actual unit kind", other)),
        };
        let source = owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root))
            .collect::<Vec<_>>();
        let returning = matches!(kind, UE::Return(_));
        need(
            source.len() == if returning { 2 } else { 1 }
                && source.iter().filter(|d| d.point() == lexical).count() == 1
                && (!returning || source.iter().filter(|d| d.point() == point).count() == 1)
                && source.iter().all(|d| {
                    d.value_origin() == site.root.marker
                        && (d.point() == lexical || returning && d.point() == point)
                }),
            "Resource qualified exact lexical and optional Return Drop points",
            &source,
        )?;
        let guard = one(
            owned
                .formed_owned_locals()
                .iter()
                .copied()
                .filter(|id| marker(names, *id, site.guard.marker, SymbolKind::Variable)),
            "Resource qualified actual Guard formation",
        )?;
        let guard = *one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(guard)),
            "Resource qualified actual Guard Drop",
        )?;
        need(
            guard.point() == point && guard.value_origin() == site.guard.marker,
            "Resource qualified original Guard point",
            guard,
        )?;
        need(
            p.exits().len() == 2,
            "Resource qualified complete exits",
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
                "Resource qualified actual exit",
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
            if !body || matches!(kind, UE::Break(_) | UE::Return(_)) {
                expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
            }
            expected.extend(
                source
                    .iter()
                    .filter(|d| d.point() == point)
                    .map(|d| UA::Drop(**d)),
            );
            need(
                exit.actions() == expected
                    && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
                "Resource qualified complete real cleanup sequence",
                (exit, expected),
            )?;
        }
        Ok(())
    }
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub form: usize,
    pub exit: usize,
    pub nullable: bool,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for form in 0..3 {
            for exit in 0..4 {
                for nullable in [false, true] {
                    cases.push(source(mutable, form, exit, nullable));
                }
            }
        }
    }
    assert_eq!(cases.len(), 48);
    assert_eq!(
        cases
            .iter()
            .map(|c| c.label.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        48
    );
    for mutable in [false, true] {
        for form in 0..3 {
            for exit in 0..4 {
                assert_eq!(
                    cases
                        .iter()
                        .filter(|c| c.mutable == mutable && c.form == form && c.exit == exit)
                        .count(),
                    2
                );
            }
        }
    }
    cases
}

fn source(mutable: bool, form: usize, exit: usize, nullable: bool) -> Case {
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
    let value_read = if nullable {
        "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}"
    } else {
        "println(\"value\")"
    };
    let transfer = ["", "continue", "break", "return result(key,value)"][exit];
    let mut scanners = String::new();
    for empty in [false, true] {
        let scan = if empty { "scanEmpty" } else { "scan" };
        let producer = if empty { "makeEmpty" } else { "make" };
        let declaration = match form {
            0 => format!("val source={producer}()"),
            1 => format!("var source={producer}()"),
            _ => format!("val original={producer}();val source=original"),
        };
        scanners.push_str(&format!(
            "fun {scan}():Int{{{declaration};println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};println(\"after\");return 0}}\n"
        ));
    }
    let source = format!(
        r#"class Resource(val tag:String){{deinit(){{println(this.tag)}}}}
class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:String,value:{payload}):Unit{{println(key);{value_read}}}
fun result(key:String,value:{payload}):Int{{println("return-value");read(key,value);return 7}}
fun make():{kind}<String,{payload}>{{var source=mutableMapOf<String,{payload}>();source.put("z".clone(),Resource("resource-drop".clone()));{middle}source.put("a".clone(),Resource("resource-drop".clone()));{conversion}}}
fun makeEmpty():{kind}<String,{payload}>{{return {factory}<String,{payload}>()}}
{scanners}
fun entry():Unit{{println("nonempty");val first=scan();if(first==7){{println("seven")}}else{{println("zero")}};println("empty");val second=scanEmpty();if(second==7){{println("seven")}}else{{println("zero")}};println("entry-end")}}
"#
    );
    let mut golden = String::from("nonempty\nformed\n");
    let entries: &[(&str, &str)] = if nullable {
        &[("z", "some"), ("n", "none"), ("a", "some")]
    } else {
        &[("z", "value"), ("a", "value")]
    };
    for &(key, value) in entries
        .iter()
        .take(if exit >= 2 { 1 } else { entries.len() })
    {
        golden.push_str(&format!("{key}\n{value}\n"));
        if exit == 3 {
            golden.push_str(&format!("return-value\n{key}\n{value}\n"));
        }
        golden.push_str("guard-drop\n");
    }
    if exit != 3 {
        golden.push_str("after\n");
    }
    golden.push_str("resource-drop\nresource-drop\n");
    golden.push_str(if exit == 3 { "seven\n" } else { "zero\n" });
    golden.push_str("empty\nformed\nafter\nzero\nentry-end\n");
    Case {
        label: format!(
            "local-resource-{kind}-{}-{}-{}",
            ["val", "var", "moved-alias"][form],
            ["normal", "continue", "break", "return"][exit],
            if nullable { "nullable" } else { "direct" }
        ),
        source,
        golden,
        mutable,
        form,
        exit,
        nullable,
    }
}

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "G2 RESOURCE INPUT case={} unit={unit} source={:?} golden={:?}",
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
                    "G2 Resource two actual qualified Map plans",
                    typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
                    (typed.map_iterations(), owned.map_iterations()),
                );
                diagnostic!(
                    "G2 RESOURCE UNIT case={} nullable={} formed={:#?} plans={:#?} drops={:#?}",
                    case.label,
                    case.nullable,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops()
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
                    "G2 Resource two actual qualified Map plans",
                    typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
                    (typed.map_iterations(), owned.map_iterations()),
                );
                diagnostic!(
                    "G2 RESOURCE UNIT case={} nullable={} formed={:#?} plans={:#?} drops={:#?}",
                    case.label,
                    case.nullable,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops()
                );
            },
        )
    };
    diagnostic!(
        "G2 RESOURCE REPORT {} unit={unit} report={report:#?}",
        case.label
    );
    let (program, _) = result?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    if !report.passed() {
        return Err(failure("original Resource FE queries", report));
    }
    graph::check(case, &sources, &parsed, &program, true)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}

#[test]
fn resource_locals_keep_actual_formation_and_cleanup() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            diagnostic!(
                "G2 RESOURCE CASE {} unit={unit} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert_diagnostic!(
        outcomes.len() == 96 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}
