//! Legal source replacement histories: ordinary FE/lowering observations precede repairs.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod boundaries {
    //! Ordinary legal neighbors distinguish transparent RHS groups from unproved epoch arity.
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
    // The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
    use super::*;

    pub(crate) fn grouped_sources() -> Vec<Case> {
        let cases = super::sources()
            .into_iter()
            .filter(|case| case.history == 1)
            .map(|mut case| {
                assert_eq!(case.source.matches("source=other()").count(), 1);
                assert_eq!(case.source.matches("source=otherEmpty()").count(), 1);
                case.source = case
                    .source
                    .replace("source=other()", "source=(((other())))")
                    .replace("source=otherEmpty()", "source=(((otherEmpty())))");
                case.label.push_str("-three-rhs-groups");
                case
            })
            .collect::<Vec<_>>();
        assert_eq!(cases.len(), 6);
        cases
    }

    fn arity_sources() -> Vec<(&'static str, String)> {
        let prefix = r#"class Resource{deinit(){println("resource-drop")}}
    class Guard{deinit(){println("guard-drop")}}
    fun read(key:String,value:Resource):Unit{println(key);println("value")}
    fun make():Map<String,Resource>{var source=mutableMapOf<String,Resource>();source.put("k".clone(),Resource());val result=source.consume();return result}
    fun makeWithArg(number:Int):Map<String,Resource>{val actual:Int=number;println("arity");return make()}
    "#;
        [
            ("future-arity-epoch", "var source=make();for((key,value) in source){val local=Guard();read(key,value)};println(\"between\");source=makeWithArg(1);println(\"tail\")"),
            ("previous-arity-epoch", "var source=makeWithArg(1);println(\"formed\");source=make();for((key,value) in source){val local=Guard();read(key,value)};println(\"tail\")"),
        ].into_iter().map(|(label, body)| {
            (label, format!("{prefix}fun scan():Unit{{{body}}}\nfun entry():Unit{{scan()}}\n"))
        }).collect()
    }

    fn observe(label: &str, source: &str, grouped: Option<&Case>, unit: bool) -> Result<(), Failure> {
        let golden = grouped.map(|case| case.golden.as_str());
        diagnostic!(
            "LOCAL REASSIGN NEIGHBOR INPUT case={label} unit={unit} source={source:?} golden={golden:?}"
        );
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("provider.ko", source)
            .map_err(|e| failure("source", e))?;
        let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
        clean(&sources, "lex", lexed.diagnostics())?;
        let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
        clean(&sources, "parse", parsed.diagnostics())?;
        let mut report = Report::default();
        let expected = if golden.is_some() { 4 } else { 1 };
        // Keep public validation and ordinary lowering independent: a query result
        // is recorded, never returned early instead of the actual lowering outcome.
        let lowered = if unit {
            pipeline::unit_observed(
                &sources,
                &parsed,
                "provider.ko",
                &mut report,
                |names, typed, owned, report| {
                    source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                    if let Some(case) = grouped {
                        report.query(
                            "grouped RHS original qualified epoch identity",
                            &super::facts::unit(case, &sources, &parsed, names, typed, owned),
                        );
                    }
                    report.check(
                        "neighbor original qualified descriptor count",
                        typed.map_iterations().len() == expected
                            && owned.map_iterations().len() == expected,
                        (typed.map_iterations(), owned.map_iterations()),
                    );
                    diagnostic!(
                        "LOCAL REASSIGN NEIGHBOR UNIT case={label} plans={:#?} drops={:#?}",
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
                    if let Some(case) = grouped {
                        report.query(
                            "grouped RHS original qualified epoch identity",
                            &super::facts::unit(case, &sources, &parsed, names, typed, owned),
                        );
                    }
                    report.check(
                        "neighbor original qualified descriptor count",
                        typed.map_iterations().len() == expected
                            && owned.map_iterations().len() == expected,
                        (typed.map_iterations(), owned.map_iterations()),
                    );
                    diagnostic!(
                        "LOCAL REASSIGN NEIGHBOR UNIT case={label} plans={:#?} drops={:#?}",
                        owned.map_iterations(),
                        owned.drops()
                    );
                },
            )
        };
        diagnostic!(
            "LOCAL REASSIGN NEIGHBOR LOWER case={label} unit={unit} report={report:#?} outcome={:?}",
            lowered.as_ref().map(|(_, entry)| entry)
        );
        let (program, _) = lowered?;
        report.query(
            "Unit complete temporary source acquisition",
            &source_forms::graph::temporary_entries(&parsed, &program),
        );
        if let Some(case) = grouped {
            report.query(
                "grouped RHS independent SSA identities",
                &super::graph::check(case, &sources, &parsed, &program, true),
            );
        }
        let verified =
            crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e));
        report.query("neighbor actual SSA verifier", &verified);
        let llvm =
            crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e));
        report.query("neighbor actual LLVM verifier", &llvm);
        diagnostic!("LOCAL REASSIGN NEIGHBOR FINAL case={label} unit={unit} report={report:#?}");
        if report.passed() {
            Ok(())
        } else {
            Err(failure("neighbor observation failed", report))
        }
    }

    #[test]
    fn grouped_replacement_rhs_preserves_both_verified_pipelines() {
        let mut observations = Vec::new();
        for case in grouped_sources() {
            for unit in [false, true] {
                let result = observe(&case.label, &case.source, Some(&case), unit);
                diagnostic!(
                    "LOCAL REASSIGN GROUP CASE {} unit={unit} result={result:?}",
                    case.label
                );
                observations.push((case.label.clone(), unit, result));
            }
        }
        assert_eq!(observations.len(), 12);
        assert_diagnostic!(
            observations.iter().all(|(_, _, result)| result.is_ok()),
            "{observations:#?}"
        );
    }

    #[test]
    fn ordinary_other_epoch_arguments_keep_validated_call_delivery() {
        let mut observations = Vec::new();
        for (label, source) in arity_sources() {
            for unit in [false, true] {
                let result = observe(label, &source, None, unit);
                diagnostic!("LOCAL REASSIGN ARITY CASE {label} unit={unit} result={result:?}");
                observations.push((label, unit, result));
            }
        }
        assert_eq!(observations.len(), 4);
        assert_diagnostic!(
            observations.iter().all(|(_, _, result)| result.is_ok()),
            "{observations:#?}"
        );
    }

    pub(crate) fn ordinary_epoch_sources() -> Vec<(&'static str, String, &'static str)> {
        arity_sources()
            .into_iter()
            .map(|(label, source)| {
                let golden = if label == "future-arity-epoch" {
                    "k\nvalue\nguard-drop\nbetween\narity\nresource-drop\ntail\nresource-drop\n"
                } else {
                    "arity\nformed\nresource-drop\nk\nvalue\nguard-drop\ntail\nresource-drop\n"
                };
                (label, source, golden)
            })
            .collect()
    }
}
mod completed_epoch;
mod facts {
    //! Exact same-analysis assignment epochs; declaration history never substitutes for current birth.
    pub(super) mod ast {
        //! Original assignments, epoch births and loop identities shared by independent FE/SSA queries.
        use super::*;
        use lang_frontend::{
            ast::{ExpressionId, StatementId},
            parser::{
                AssignmentOperator, Expression, FunctionBody, FunctionForm, Item, NameMarker, Statement,
                VariableKind,
            },
            source::Span,
        };
        #[derive(Debug)]
        pub(in super::super) struct Loop {
            pub span: Span,
            pub statement: StatementId,
            pub source: ExpressionId,
            pub body: StatementId,
            pub epoch: usize,
            pub marker: Span,
        }
        #[derive(Debug)]
        pub(in super::super) struct Site {
            pub scan: Span,
            pub scope: StatementId,
            pub declaration: StatementId,
            pub binding: Span,
            pub birth: ExpressionId,
            pub assignment: ExpressionId,
            pub target: ExpressionId,
            pub rhs_outer: ExpressionId,
            pub rhs: ExpressionId,
            pub alias: Option<(Span, ExpressionId)>,
            pub factories: [Span; 2],
            pub loops: Vec<Loop>,
        }
        pub(in super::super) fn inside(span: Span, outer: Span) -> bool {
            span.source_id() == outer.source_id()
                && span.start() > outer.start()
                && span.end() < outer.end()
        }
        pub(in super::super) fn expression(parsed: &ParsedFile, id: ExpressionId) -> Span {
            parsed.ast().expressions().get(id).unwrap().span()
        }
        pub(in super::super) fn declaration(
            sources: &SourceMap,
            parsed: &ParsedFile,
            wanted: &str,
        ) -> Result<Span, Failure> {
            one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok(wanted)).then_some(n.span())}),"actual original factory/scanner declaration")
        }
        pub(in super::super) fn sites(
            case: &Case,
            sources: &SourceMap,
            parsed: &ParsedFile,
        ) -> Result<Vec<Site>, Failure> {
            [false,true].into_iter().map(|empty|{
                let scan=declaration(sources,parsed,if empty{"scanEmpty"}else{"scan"})?;
                let node=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|n.span()==scan),"actual scanner item")?;
                let Item::Function{parameters,form,..}=node.payload() else{unreachable!()};
                need(parameters.is_empty(),"actual zero parameter scanner",node)?;
                let scope=match form{FunctionForm::ImplicitUnitBlock(b)|FunctionForm::Explicit{body:FunctionBody::Block(b),..}=>*b,_=>return Err(failure("actual scanner scope",form))};
                let (local,decl)=one(parsed.ast().statements().iter().filter_map(|(id,n)|{let Statement::LocalVariable{declaration}=n.payload() else{return None;};let item=parsed.ast().items().get(*declaration).ok()?;matches!(item.payload(),Item::Variable{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("source") && inside(n.span(),scan)).then_some((item,id))}),"actual source declaration statement")?;
                let Item::Variable{name:NameMarker::Present(binding),initializer:birth,kind:VariableKind::Var,..}=local.payload() else{return Err(failure("original var source initializer",local))};
                let (assignment,target,rhs)=one(parsed.ast().expressions().iter().filter_map(|(id,n)|{let Expression::Assignment{operator:AssignmentOperator::Assign,target,value,..}=n.payload() else{return None;};(inside(n.span(),scan) && sources.slice(expression(parsed,*target))==Ok("source")).then_some((id,*target,*value))}),"actual replacement RHS expression")?;
                let rhs_outer=rhs;
                let rhs=*rhs_chain(parsed,rhs_outer).last().unwrap();
                let alias=parsed.ast().items().iter().filter_map(|(_,n)|{let Item::Variable{name:NameMarker::Present(name),initializer,kind:VariableKind::Val,..}=n.payload() else{return None;};(inside(n.span(),scan) && sources.slice(*name)==Ok("alias")).then_some((*name,*initializer))}).collect::<Vec<_>>();
                need(alias.len()==usize::from(case.history==2),"exact moved alias history",&alias)?;
                if let Some((_,init))=alias.first(){need(sources.slice(expression(parsed,*init))==Ok("source") && expression(parsed,*init).start()>expression(parsed,assignment).end(),"alias moves replacement epoch only",init)?;}
                let factories=[declaration(sources,parsed,if empty{"makeEmpty"}else{"make"})?,declaration(sources,parsed,if empty{"otherEmpty"}else{"other"})?];
                for (id,wanted) in [(*birth,if empty{"makeEmpty"}else{"make"}),(rhs,if empty{"otherEmpty"}else{"other"})]{let n=parsed.ast().expressions().get(id).unwrap();let Expression::Call{callee,arguments,..}=n.payload() else{return Err(failure("original complete factory Call",n))};need(arguments.is_empty() && sources.slice(expression(parsed,*callee))==Ok(wanted),"actual distinct epoch producer AST",n)?;}
                let mut loops=parsed.ast().statements().iter().filter_map(|(id,n)|{let Statement::For{source,body,..}=n.payload() else{return None;};inside(n.span(),scan).then_some((id,n.span(),*source,*body))}).collect::<Vec<_>>();loops.sort_by_key(|(_,s,_,_)|s.start());
                need(loops.len()==if case.history==1{2}else{1},"original epoch provider count",&loops)?;
                let loops=loops.into_iter().map(|(statement,span,source,body)|{
                    let epoch=usize::from(span.start()>expression(parsed,assignment).end());let marker=if epoch==1{alias.first().map_or(*binding,|(name,_)|*name)}else{*binding};
                    need(sources.slice(expression(parsed,source))==Ok(if epoch==1 && case.history==2{"alias"}else{"source"}),"actual epoch source reference",source)?;
                    Ok(Loop{statement,span,source,body,epoch,marker})
                }).collect::<Result<Vec<_>,Failure>>()?;
                need(loops.iter().map(|l|l.epoch).collect::<Vec<_>>()==if case.history==1{vec![0,1]}else{vec![1]},"old/new loop chronology",&loops)?;
                Ok(Site{scan,scope,declaration:decl,binding:*binding,birth:*birth,assignment,target,rhs_outer,rhs,alias:alias.first().copied(),factories,loops})
            }).collect()
        }

        // Keep every real Group ID; only the final actual Call may grant producer identity.
        pub(in super::super) fn rhs_chain(parsed: &ParsedFile, outer: ExpressionId) -> Vec<ExpressionId> {
            let mut chain = vec![outer];
            while let Expression::Group { expression } = parsed
                .ast()
                .expressions()
                .get(*chain.last().unwrap())
                .unwrap()
                .payload()
            {
                chain.push(*expression);
            }
            chain
        }
    }
    use super::Case;
    use crate::ssa::map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use lang_frontend::{
        name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
        ownership_checking::{
            CompilationUnitOwnership, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
            UnitIterationExitKind as UE, UnitIterationSourceAccess,
        },
        parser::ParsedFile,
        source::SourceMap,
        type_checking::{CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId},
    };

    fn unit_reference(
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        id: lang_frontend::ast::ExpressionId,
    ) -> Result<UnitSymbolId, Failure> {
        one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::expression(parsed, id))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                }),
            "actual qualified source/root Name identity",
        )
    }

    pub(super) fn unit(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        for site in ast::sites(case, sources, parsed)? {
            let root = unit_reference(parsed, names, site.target)?;
            let u = root.source_unit();
            need(
                owned.formed_owned_locals().contains(&root),
                "qualified original formed root",
                root,
            )?;
            if let Some((_, init)) = site.alias {
                need(
                    unit_reference(parsed, names, init)? == root,
                    "qualified alias original root",
                    init,
                )?;
            }
            let rhs_type = typed
                .expression_type(UnitExpressionId::new(u, site.rhs))
                .ok_or_else(|| failure("qualified canonical RHS type", site.rhs))?;
            let chain = ast::rhs_chain(parsed, site.rhs_outer);
            need(
                chain.last() == Some(&site.rhs),
                "qualified complete Group chain reaches canonical Call",
                &chain,
            )?;
            for id in chain {
                let id = UnitExpressionId::new(u, id);
                need(
                    typed.expression_type(id) == Some(rhs_type)
                        && typed.expression_category(id)
                            == Some(lang_frontend::type_checking::ExpressionCategory::Temporary),
                    "every qualified RHS Group layer preserves represented type and Temporary category",
                    id,
                )?;
            }
            for (epoch, id) in [site.birth, site.rhs].into_iter().enumerate() {
                let id = UnitExpressionId::new(u, id);
                let call = typed
                    .call(id)
                    .ok_or_else(|| failure("qualified epoch Call", id))?;
                let UnitCallTarget::Declaration(decl) = call.target() else {
                    return Err(failure("qualified source factory", call));
                };
                let signature = typed
                    .signatures()
                    .declaration(decl)
                    .and_then(|s| s.callable())
                    .ok_or_else(|| failure("actual factory signature", decl))?;
                need(
                    site.factories[epoch].start() <= signature.name_span().start()
                        && signature.name_span().end() <= site.factories[epoch].end()
                        && call.receiver().is_none()
                        && call.arguments().is_empty()
                        && typed.expression_type(id) == Some(call.return_type()),
                    "qualified epoch Call selected original declaration",
                    call,
                )?;
            }
            let alias = unit_reference(parsed, names, site.loops.last().unwrap().source)?;
            let roots = [root, alias];
            let drops = owned
                .drops()
                .iter()
                .filter(|f| matches!(f.target(),UnitDropTarget::Named(r) if roots.contains(&r)))
                .collect::<Vec<_>>();
            need(
                drops.len() == 2,
                "qualified old/new obligations no alias duplicate",
                &drops,
            )?;
            let old_point = if case.payload != 0 {
                UnitDropPoint::AfterExpression(UnitExpressionId::new(u, site.rhs_outer))
            } else if case.history == 1 {
                UnitDropPoint::LoopExit(UnitStatementId::new(u, site.loops[0].statement))
            } else {
                UnitDropPoint::AfterStatement(UnitStatementId::new(u, site.declaration))
            };
            let last = site.loops.last().unwrap();
            let new_point = if case.payload != 0 {
                UnitDropPoint::AfterStatement(UnitStatementId::new(u, site.scope))
            } else {
                UnitDropPoint::LoopExit(UnitStatementId::new(u, last.statement))
            };
            let old = one(
                drops.iter().copied().filter(|d| d.point() == old_point),
                "qualified old epoch point",
            )?;
            let new = one(
                drops.iter().copied().filter(|d| d.point() == new_point),
                "qualified new epoch point",
            )?;
            let new_origin = if case.history == 2 {
                last.marker
            } else {
                ast::expression(parsed, site.assignment)
            };
            need(
                old.target() == UnitDropTarget::Named(root)
                    && old.value_origin() == site.binding
                    && new.target() == UnitDropTarget::Named(alias)
                    && new.value_origin() == new_origin,
                "qualified actual assignment/alias epoch origin",
                (old, new),
            )?;
            for l in &site.loops {
                let statement = UnitStatementId::new(u, l.statement);
                let p = owned
                    .map_iteration(statement)
                    .ok_or_else(|| failure("qualified epoch plan", statement))?;
                let d = p.descriptor();
                let lr = unit_reference(parsed, names, l.source)?;
                need(
                    typed.map_iteration(statement) == Some(d)
                        && d.source() == UnitExpressionId::new(u, l.source)
                        && p.source_access() == UnitIterationSourceAccess::Owned
                        && owned.formed_owned_locals().contains(&lr),
                    "qualified epoch descriptor identity",
                    p,
                )?;
                need(
                    matches!(p.source(), Some(lang_frontend::ownership_checking::UnitLoanTarget::Place(place)) if place.is_root() && place.root()==lr)
                        && typed.expression_type(UnitExpressionId::new(
                            u,
                            if l.epoch == 0 { site.birth } else { site.rhs },
                        )) == Some(d.source_type())
                        && p.loans().len() == 2
                        && p.loans().iter().enumerate().all(|(index, loan)| {
                            loan.statement() == statement
                                && loan.source() == d.source()
                                && loan.binding() == d.bindings()[index]
                                && p.bindings()[index].kind()
                                    == lang_frontend::ownership_checking::OwnershipBindingKind::Shared
                        }),
                    "qualified epoch original root/RHS type and Shared component loans",
                    p,
                )?;
                need(p.exits().len() == 2, "qualified complete epoch exits", p)?;
                for (kind, point, body) in [
                    (
                        UE::Fallthrough,
                        UnitDropPoint::AfterStatement(UnitStatementId::new(u, l.body)),
                        true,
                    ),
                    (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
                ] {
                    let exit = one(
                        p.exits()
                            .iter()
                            .filter(|e| e.kind() == kind && e.point() == point),
                        "qualified exact normal exit",
                    )?;
                    let mut expected = owned
                        .drops()
                        .iter()
                        .copied()
                        .filter(|f| {
                            f.point() == point
                                && f.value_origin().start() > l.span.start()
                                && f.value_origin().end() < l.span.end()
                        })
                        .map(UA::Drop)
                        .collect::<Vec<_>>();
                    if body {
                        need(
                            expected.len() == 1,
                            "qualified actual Guard prefix",
                            &expected,
                        )?;
                        expected.extend([
                            UA::EndBinding {
                                statement,
                                symbol: d.value_binding(),
                            },
                            UA::EndBinding {
                                statement,
                                symbol: d.key_binding(),
                            },
                        ]);
                    } else {
                        expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
                        expected.extend(
                            owned
                                .drops()
                                .iter()
                                .copied()
                                .filter(|f| {
                                    f.point() == point
                                        && !(f.value_origin().start() > l.span.start()
                                            && f.value_origin().end() < l.span.end())
                                })
                                .map(UA::Drop),
                        );
                    }
                    need(
                        exit.actions() == expected
                            && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
                        "qualified full cleanup order",
                        exit,
                    )?;
                }
            }
        }
        Ok(())
    }
}
mod graph;
mod ordinary_routes;
mod sources {
    //! Distinct real producers and payloads expose stale roots and replacement cleanup ordering.
    pub(crate) struct Case {
        pub label: String,
        pub source: String,
        pub golden: String,
        pub mutable: bool,
        pub payload: usize,
        pub history: usize,
    }

    pub(crate) fn sources() -> Vec<Case> {
        let mut cases = Vec::new();
        for mutable in [false, true] {
            for payload in 0..3 {
                for history in 0..3 {
                    cases.push(source(mutable, payload, history));
                }
            }
        }
        assert_eq!(cases.len(), 18);
        assert_eq!(
            cases
                .iter()
                .map(|case| &case.label)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            18
        );
        cases
    }

    fn source(mutable: bool, payload: usize, history: usize) -> Case {
        let kind = if mutable { "MutableMap" } else { "Map" };
        let value = ["String", "Resource", "Resource?"][payload];
        let conversion = if mutable {
            "return source"
        } else {
            "val result=source.consume();return result"
        };
        let mut factories = String::new();
        for (name, tag, empty) in [
            ("make", "old", false),
            ("other", "new", false),
            ("makeEmpty", "old", true),
            ("otherEmpty", "new", true),
        ] {
            let item = if payload == 0 {
                format!("\"{tag}-value\".clone()")
            } else {
                format!("Resource(\"{tag}-drop\".clone())")
            };
            let entries = if empty {
                String::new()
            } else {
                let mut entries = format!("source.put(\"{tag}-key\".clone(),{item});");
                if payload == 2 {
                    entries.push_str(&format!("source.put(\"{tag}-null\".clone(),null);"));
                }
                entries
            };
            factories.push_str(&format!("fun {name}():{kind}<String,{value}>{{println(\"{name}-start\");var source=mutableMapOf<String,{value}>();{entries}println(\"{name}-ready\");{conversion}}}\n"));
        }
        let read_value = match payload {
            0 => "println(value)",
            1 => "println(\"resource-borrow\")",
            _ => "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}",
        };
        let mut scanners = String::new();
        for empty in [false, true] {
            let scan = if empty { "scanEmpty" } else { "scan" };
            let make = if empty { "makeEmpty" } else { "make" };
            let other = if empty { "otherEmpty" } else { "other" };
            let first = if history == 1 {
                "for((key,value) in source){val local=Guard();read(key,value)};println(\"between\");"
            } else {
                ""
            };
            let alias = if history == 2 {
                "val alias=source;println(\"moved\");"
            } else {
                ""
            };
            let root = if history == 2 { "alias" } else { "source" };
            scanners.push_str(&format!("fun {scan}():Unit{{var source={make}();println(\"formed\");{first}source={other}();println(\"replaced\");{alias}for((key,value) in {root}){{val local=Guard();read(key,value)}};println(\"tail\")}}\n"));
        }
        let source = format!(
            r#"class Resource(val tag:String){{deinit(){{println(this.tag)}}}}
    class Guard{{deinit(){{println("guard-drop")}}}}
    fun read(key:String,value:{value}):Unit{{println(key);{read_value}}}
    {factories}
    {scanners}
    fun entry():Unit{{println("nonempty");scan();println("empty");scanEmpty();println("entry-end")}}
    "#
        );
        let mut golden = String::new();
        for empty in [false, true] {
            let make = if empty { "makeEmpty" } else { "make" };
            let other = if empty { "otherEmpty" } else { "other" };
            golden.push_str(if empty { "empty\n" } else { "nonempty\n" });
            golden.push_str(&format!("{make}-start\n{make}-ready\nformed\n"));
            if history == 1 {
                if !empty {
                    iteration(&mut golden, "old", payload);
                }
                golden.push_str("between\n");
            }
            golden.push_str(&format!("{other}-start\n{other}-ready\n"));
            if !empty && payload != 0 {
                golden.push_str("old-drop\n");
            }
            golden.push_str("replaced\n");
            if history == 2 {
                golden.push_str("moved\n");
            }
            if !empty {
                iteration(&mut golden, "new", payload);
            }
            golden.push_str("tail\n");
            if !empty && payload != 0 {
                golden.push_str("new-drop\n");
            }
        }
        golden.push_str("entry-end\n");
        Case {
            label: format!(
                "local-reassignment-{kind}-{}-{}",
                ["pure", "resource-field", "nullable-field"][payload],
                ["before-for", "between-fors", "then-move"][history]
            ),
            source,
            golden,
            mutable,
            payload,
            history,
        }
    }

    fn iteration(golden: &mut String, tag: &str, payload: usize) {
        golden.push_str(&format!("{tag}-key\n"));
        golden.push_str(match payload {
            0 => {
                if tag == "old" {
                    "old-value\n"
                } else {
                    "new-value\n"
                }
            }
            1 => "resource-borrow\n",
            _ => "some\n",
        });
        golden.push_str("guard-drop\n");
        if payload == 2 {
            golden.push_str(&format!("{tag}-null\nnone\nguard-drop\n"));
        }
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
pub(crate) use boundaries::grouped_sources;
pub(crate) use completed_epoch::sources as completed_epoch_sources;
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
pub(crate) use ordinary_routes::{ordinary_route_controls, ordinary_route_sources};
pub(crate) use sources::{Case, sources};

fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "LOCAL REASSIGN INPUT case={} unit={unit} source={:?} golden={:?}",
        case.label,
        case.source,
        case.golden
    );
    diagnostic!(
        "LOCAL REASSIGN AXES mutable={} payload={} history={}",
        case.mutable,
        case.payload,
        case.history
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
    let expected = if case.history == 1 { 4 } else { 2 };
    // Public identity queries are recorded before lowering and never short-circuit
    // that lowering. Failed compilation remains the actual observed error.
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |names, typed, owned, report| {
                source_forms::facts::unit(&sources, &parsed, names, typed, owned, report);
                report.query(
                    "reassignment independent unit facts",
                    &facts::unit(case, &sources, &parsed, names, typed, owned),
                );
                report.check(
                    "reassignment original qualified descriptor count",
                    typed.map_iterations().len() == expected,
                    typed.map_iterations(),
                );
                diagnostic!(
                    "LOCAL REASSIGN UNIT FACTS case={} formed={:#?} plans={:#?} drops={:#?} deliveries={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.value_deliveries(),
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
                report.query(
                    "reassignment independent unit facts",
                    &facts::unit(case, &sources, &parsed, names, typed, owned),
                );
                report.check(
                    "reassignment original qualified descriptor count",
                    typed.map_iterations().len() == expected,
                    typed.map_iterations(),
                );
                diagnostic!(
                    "LOCAL REASSIGN UNIT FACTS case={} formed={:#?} plans={:#?} drops={:#?} deliveries={:#?} loans={:#?}",
                    case.label,
                    owned.formed_owned_locals(),
                    owned.map_iterations(),
                    owned.drops(),
                    owned.value_deliveries(),
                    owned.loans()
                );
            },
        )
    };
    diagnostic!(
        "LOCAL REASSIGN LOWER case={} unit={unit} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    report.query(
        "reassignment independent SSA identities",
        &graph::check(case, &sources, &parsed, &program, true),
    );
    let verified =
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA verifier", e));
    report.query("reassignment actual SSA verifier", &verified);
    let llvm =
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM verifier", e));
    report.query("reassignment actual LLVM verifier", &llvm);
    diagnostic!(
        "LOCAL REASSIGN FINAL case={} unit={unit} report={report:#?}",
        case.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("reassignment observed stage failures", report))
    }
}

#[test]
fn reassigned_sources_preserve_actual_value_histories() {
    let mut observations = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            diagnostic!(
                "LOCAL REASSIGN CASE {} unit={unit} result={result:?}",
                case.label
            );
            observations.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(observations.len(), 36);
    assert_diagnostic!(
        observations.iter().all(|(_, _, result)| result.is_ok()),
        "{observations:#?}"
    );
}

pub(crate) use boundaries::ordinary_epoch_sources;
