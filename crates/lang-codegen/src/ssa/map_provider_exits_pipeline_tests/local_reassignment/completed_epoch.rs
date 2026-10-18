//! The installation event and its complete RHS share an end byte, but are different epochs.
use super::super::{
    Report,
    fixture::{Failure, clean, failure, need},
    pipeline,
    query_inputs::one,
};
use crate::ssa::{
    map_provider_exits_pipeline_tests::query_graph::{Graph, Uses, loan, result, value},
    model::*,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    lexer::lex,
    name_resolution::{
        NameResolution, ReferenceTarget, UnitReferenceTarget, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition as C, CleanupOwnerValue as O, CompilationUnitOwnership, DropPoint,
        DropTarget, IterationPhiBoundary as B, IterationPhiIncomingKind as K, OwnershipCheckedFile,
        UnitDropPoint, UnitDropTarget,
    },
    parser::{Expression, Item, NameMarker, ParsedFile, Statement, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, ExpressionCategory, TypedFile, UnitExpressionId, UnitStatementId,
    },
};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub grouped: bool,
}
pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for grouped in [false, true] {
            let kind = if mutable { "MutableMap" } else { "Map" };
            let mut factories = String::new();
            for (name, tag, empty) in [
                ("make", "old", false),
                ("other", "new", false),
                ("makeEmpty", "old", true),
                ("otherEmpty", "new", true),
            ] {
                let put = if empty {
                    String::new()
                } else {
                    format!("source.put(\"{tag}-key\".clone(),\"{tag}-value\".clone());")
                };
                factories.push_str(&format!("fun {name}():{kind}<String,String>{{println(\"{name}-start\");var source=mutableMapOf<String,String>();{put}println(\"{name}-ready\");{}}}\n",if mutable{"return source"}else{"val result=source.consume();return result"}));
            }
            let mut scanners = String::new();
            for empty in [false, true] {
                let (scan, make, other) = if empty {
                    ("scanEmpty", "makeEmpty", "otherEmpty")
                } else {
                    ("scan", "make", "other")
                };
                let rhs = if grouped {
                    format!("((({other}())))")
                } else {
                    format!("{other}()")
                };
                scanners.push_str(&format!("fun {scan}():Unit{{var source={make}();println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value)}};println(\"between\");source={rhs};println(\"tail\")}}\n"));
            }
            let source = format!(
                "class Guard{{deinit(){{println(\"guard-drop\")}}}}\nfun read(key:String,value:String):Unit{{println(key);println(value)}}\n{factories}{scanners}fun entry():Unit{{println(\"nonempty\");scan();println(\"empty\");scanEmpty();println(\"entry-end\")}}\n"
            );
            let golden="nonempty\nmake-start\nmake-ready\nformed\nold-key\nold-value\nguard-drop\nbetween\nother-start\nother-ready\ntail\nempty\nmakeEmpty-start\nmakeEmpty-ready\nformed\nbetween\notherEmpty-start\notherEmpty-ready\ntail\nentry-end\n".to_owned();
            cases.push(Case {
                label: format!(
                    "completed-epoch-{kind}-{}",
                    if grouped { "triple-group" } else { "plain" }
                ),
                source,
                golden,
                mutable,
                grouped,
            });
        }
    }
    assert_eq!(cases.len(), 4);
    cases
}
struct Site {
    scan: Span,
    binding: Span,
    old: ExpressionId,
    assignment: ExpressionId,
    rhs: ExpressionId,
    call: ExpressionId,
    statement: StatementId,
    source: ExpressionId,
    body: StatementId,
}
fn inside(at: Span, outer: Span) -> bool {
    at.source_id() == outer.source_id() && at.start() > outer.start() && at.end() < outer.end()
}
fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}
fn sites(sources: &SourceMap, parsed: &ParsedFile) -> Result<Vec<Site>, Failure> {
    ["scan","scanEmpty"].into_iter().map(|wanted|{
        let scan=one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok(wanted)).then_some(n.span())}),"completed epoch original scanner")?;
        let (binding,old)=one(parsed.ast().items().iter().filter_map(|(_,n)|match n.payload(){Item::Variable{name:NameMarker::Present(name),initializer,..} if inside(n.span(),scan) && sources.slice(*name)==Ok("source")=>Some((*name,*initializer)),_=>None}),"completed epoch original local")?;
        let (assignment,rhs)=one(parsed.ast().expressions().iter().filter_map(|(id,n)|match n.payload(){Expression::Assignment{target,value,..} if inside(n.span(),scan) && sources.slice(span(parsed,*target))==Ok("source")=>Some((id,*value)),_=>None}),"completed epoch actual assignment")?;
        need(assignment!=rhs && span(parsed,assignment).end()==span(parsed,rhs).end(),"installation and complete RHS are distinct same-end events",(assignment,rhs))?;
        let mut call=rhs;while let Expression::Group{expression}=parsed.ast().expressions().get(call).unwrap().payload(){call=*expression;}
        let (statement,source,body)=one(parsed.ast().statements().iter().filter_map(|(id,n)|match n.payload(){Statement::For{source,body,..} if inside(n.span(),scan)=>Some((id,*source,*body)),_=>None}),"old epoch original provider")?;
        need(span(parsed,source).end()<span(parsed,assignment).start(),"provider precedes replacement of completed epoch",statement)?;
        Ok(Site{scan,binding,old,assignment,rhs,call,statement,source,body})
    }).collect()
}
fn single(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    for s in sites(sources, parsed)? {
        let root = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == span(parsed, s.source))
                .filter_map(|r| match r.target() {
                    ReferenceTarget::Symbol(r) => Some(*r),
                    _ => None,
                }),
            "completed epoch original source root",
        )?;
        let mut id = s.rhs;
        let ty = typed.expression_type(s.call);
        let mut depth = 0;
        loop {
            need(
                typed.expression_type(id) == ty
                    && typed.expression_category(id) == Some(ExpressionCategory::Temporary),
                "every completed RHS Group preserves actual type/category",
                id,
            )?;
            if let Expression::Group { expression } =
                parsed.ast().expressions().get(id).unwrap().payload()
            {
                depth += 1;
                id = *expression;
            } else {
                break;
            }
        }
        need(
            depth == if case.grouped { 3 } else { 0 } && id == s.call,
            "exact complete RHS canonical Call",
            id,
        )?;
        let p = owned
            .map_iteration(s.statement)
            .ok_or_else(|| failure("actual old epoch plan", s.statement))?;
        let table = owned.cleanup_conditions();
        let h = one(
            p.closure_phis()
                .iter()
                .filter(|p| p.symbol() == root && p.boundary() == B::Header),
            "old current Header",
        )?;
        let x = one(
            p.closure_phis()
                .iter()
                .filter(|p| p.symbol() == root && p.boundary() == B::Exit),
            "old current Exit",
        )?;
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(root))
            .collect::<Vec<_>>();
        let new = one(
            drops
                .iter()
                .copied()
                .filter(|d| d.point() == DropPoint::AfterExpression(s.assignment)),
            "new installation's actual ASAP Drop",
        )?;
        need(
            new.value_origin() == span(parsed, s.assignment)
                && new.condition().is_none()
                && matches!(new.owner().and_then(|v|table.owner_value(v)),Some(O::Expression{expression,origin}) if *expression==s.call && *origin==span(parsed,s.call)),
            "new Drop uses actual other Call epoch, never old declaration",
            new,
        )?;
        let entry = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == K::Entry),
            "old actual Entry",
        )?;
        let b = one(entry.bindings().iter(), "old Entry complete binding")?;
        let v = one(b.values().iter(), "old Entry actual owner")?;
        need(
            matches!(table.owner_value(v.source()),Some(O::Expression{expression,origin}) if *expression==s.old && *origin==span(parsed,s.old))
                && b.target() == h.owner(),
            "old provider originates in actual make Call",
            entry,
        )?;
        let fall = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == K::Fallthrough),
            "old actual backedge",
        )?;
        let b = one(fall.bindings().iter(), "old backedge binding")?;
        let v = one(b.values().iter(), "old backedge owner")?;
        need(
            b.target() == h.owner()
                && v.source() == h.owner()
                && v.condition() == h.availability_condition(),
            "old complete live backedge",
            fall,
        )?;
        let exhaustion = one(
            p.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == K::Exhaustion),
            "old actual exhaustion",
        )?;
        let absent = one(exhaustion.bindings().iter(), "old complete Exit binding")?;
        need(
            absent.target() == x.owner()
                && absent.values().is_empty()
                && table.get(absent.available_when()) == Some(&C::Never),
            "Pure old epoch has already been consumed before assignment",
            exhaustion,
        )?;
        let actual = one(
            drops
                .iter()
                .copied()
                .filter(|d| d.point() == DropPoint::LoopExit(s.statement)),
            "old real ASAP provider Drop",
        )?;
        need(
            actual.owner() == Some(h.owner())
                && actual.condition() == Some(h.availability_condition())
                && actual.value_origin() == s.binding,
            "old Drop consumes actual current Header",
            actual,
        )?;
        for drop in drops {
            if drop == new || drop == actual {
                continue;
            }
            need(
                drop.point() == DropPoint::AfterExpression(s.rhs)
                    && drop.value_origin() == s.binding
                    && drop.owner() == Some(x.owner())
                    && drop.condition() == Some(x.availability_condition()),
                "any residual old fact belongs only to absent Exit at complete outer RHS",
                drop,
            )?;
        }
        println!("COMPLETED EPOCH SINGLE actual-old={actual:#?} new={new:#?} absent={absent:#?}");
    }
    Ok(())
}
fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    for s in sites(sources, parsed)? {
        let root = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == span(parsed, s.source))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(r) => Some(*r),
                    _ => None,
                }),
            "qualified completed source root",
        )?;
        let u = root.source_unit();
        let mut id = s.rhs;
        let ty = typed.expression_type(UnitExpressionId::new(u, s.call));
        let mut depth = 0;
        loop {
            let q = UnitExpressionId::new(u, id);
            need(
                typed.expression_type(q) == ty
                    && typed.expression_category(q) == Some(ExpressionCategory::Temporary),
                "qualified completed Group type/category",
                q,
            )?;
            if let Expression::Group { expression } =
                parsed.ast().expressions().get(id).unwrap().payload()
            {
                depth += 1;
                id = *expression;
            } else {
                break;
            }
        }
        need(
            depth == if case.grouped { 3 } else { 0 } && id == s.call,
            "qualified canonical actual Call",
            id,
        )?;
        let drops = owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root))
            .collect::<Vec<_>>();
        let new = one(
            drops.iter().copied().filter(|d| {
                d.point() == UnitDropPoint::AfterExpression(UnitExpressionId::new(u, s.assignment))
            }),
            "qualified new installation ASAP Drop",
        )?;
        need(
            new.value_origin() == span(parsed, s.assignment),
            "qualified new value origin is complete assignment event",
            new,
        )?;
        let old = one(
            drops.iter().copied().filter(|d| {
                d.point() == UnitDropPoint::LoopExit(UnitStatementId::new(u, s.statement))
            }),
            "qualified old actual provider Drop",
        )?;
        need(
            old.value_origin() == s.binding,
            "qualified old original binding origin",
            old,
        )?;
        for d in drops {
            need(
                d == new
                    || d == old
                    || (d.point()
                        == UnitDropPoint::AfterExpression(UnitExpressionId::new(u, s.rhs))
                        && d.value_origin() == s.binding),
                "qualified old/new exact distinct cleanup events",
                d,
            )?;
        }
        println!("COMPLETED EPOCH UNIT old={old:#?} new={new:#?}");
    }
    Ok(())
}
fn observed(case: &Case, unit_mode: bool) -> Result<(), Failure> {
    println!(
        "COMPLETED EPOCH INPUT {} mutable={} grouped={} source={:?} golden={:?}",
        case.label, case.mutable, case.grouped, case.source, case.golden
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
    let lowered = if unit_mode {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |n, t, o, r| {
                println!(
                    "COMPLETED EPOCH UNIT RAW plans={:#?} drops={:#?}",
                    o.map_iterations(),
                    o.drops()
                );
                r.query(
                    "completed event qualified FE identity",
                    &unit(case, &sources, &parsed, n, t, o),
                );
            },
        )
    } else {
        pipeline::single_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            println!(
                "COMPLETED EPOCH SINGLE RAW plans={:#?} drops={:#?} conditions={:#?}",
                o.map_iterations(),
                o.drops(),
                o.cleanup_conditions()
            );
            r.query(
                "completed event single FE identity",
                &single(case, &sources, &parsed, n, t, o),
            );
        })
    };
    println!(
        "COMPLETED EPOCH LOWER {} unit={unit_mode} report={report:#?} outcome={:?}",
        case.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    let (program, _) = lowered?;
    report.query(
        "completed event actual SSA identity",
        &graph(case, &sources, &parsed, &program, unit_mode),
    );
    report.query(
        "completed event SSA verifier",
        &crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e)),
    );
    report.query(
        "completed event LLVM verifier",
        &crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e)),
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("completed event query", report))
    }
}
fn factory<'a>(
    g: &Graph<'a>,
    module: &'a Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    id: ExpressionId,
) -> Result<&'a Instruction, Failure> {
    let n = parsed.ast().expressions().get(id).unwrap();
    let Expression::Call {
        callee, arguments, ..
    } = n.payload()
    else {
        return Err(failure("actual completed producer Call", n));
    };
    need(
        arguments.is_empty(),
        "actual zero-argument completed producer",
        n,
    )?;
    let name = sources
        .slice(span(parsed, *callee))
        .map_err(|e| failure("producer name", e))?;
    let decl=one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(s),..} if sources.slice(*s)==Ok(name)).then_some(n.span())}),"selected original completed producer")?;
    let actual = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(decl)),
        "actual selected producer callee",
    )?;
    let call=one(g.instructions.iter().copied().filter(|i|i.origin==Origin::Source(n.span()) && matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==actual.id && arguments.is_empty())),"complete actual producer call")?;
    need(
        actual.return_types.as_slice()
            == [g.function.entity(result(call)?).unwrap().ty.semantic_type()],
        "completed producer actual Value return ABI",
        call,
    )?;
    if name.ends_with("Empty") {
        let f = Graph::new(actual)?;
        let made = one(
            f.instructions
                .iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapConstruct { .. })),
            "actual empty epoch construction",
        )?;
        let mut current = result(made)?;
        need(
            !f.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::MapPut { .. }
                        | Operation::HeapAllocate { .. }
                        | Operation::DirectCall { .. }
                )
            }),
            "empty producer inserts and allocates no payload",
            actual.id,
        )?;
        for i in &f.instructions {
            if let Operation::MapConsume { owner, .. } = i.operation {
                need(
                    current == EntityId::Value(owner),
                    "empty consume preserves real zero entries",
                    i,
                )?;
                current = result(i)?;
            }
        }
        need(f.blocks.iter().any(|&b|matches!(&f.term(b).unwrap().kind,TerminatorKind::Return{values} if matches!(current,EntityId::Value(v) if values.as_slice()==[v]))),"actual empty construction is returned",actual.id)?;
    }
    Ok(call)
}
fn graph(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    unit_mode: bool,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "completed event one module")?;
    for s in sites(sources, parsed)? {
        let f = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(s.scan)),
            "actual completed epoch scanner",
        )?;
        let g = Graph::new(f)?;
        need(
            g.block(f.entry_block().unwrap())?.parameters.is_empty() && f.return_types.is_empty(),
            "actual completed scanner Unit ABI",
            f.id,
        )?;
        let old = factory(&g, module, sources, parsed, s.old)?;
        let new = factory(&g, module, sources, parsed, s.call)?;
        let a = result(old)?;
        let n = result(new)?;
        need(
            a != n && matches!(a, EntityId::Value(_)) && matches!(n, EntityId::Value(_)),
            "completed assignment installs distinct actual Call result",
            (a, n),
        )?;
        let map_type = f.entity(a).unwrap().ty.semantic_type();
        need(
            f.entity(n).unwrap().ty == f.entity(a).unwrap().ty,
            "new epoch preserves Map type",
            new,
        )?;
        let (kind, key_type, value_type) = module
            .map_container(map_type)
            .ok_or_else(|| failure("actual completed Map ABI", map_type))?;
        need(
            kind == if case.mutable {
                MapContainerKind::MutableMap
            } else {
                MapContainerKind::Map
            } && module.type_kind(key_type) == Some(&SsaTypeKind::StringOwner)
                && module.type_kind(value_type) == Some(&SsaTypeKind::StringOwner),
            "actual Pure String map representation",
            map_type,
        )?;
        let for_span = parsed.ast().statements().get(s.statement).unwrap().span();
        let op = |p: fn(&Operation) -> bool| {
            one(
                g.instructions.iter().copied().filter(|i| p(&i.operation)),
                "actual unique completed provider operation",
            )
        };
        let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
        let has = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
        let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
        let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
        let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
        need(
            [first, has, key, val, next]
                .iter()
                .all(|i| i.origin == Origin::Source(for_span)),
            "original completed epoch For origins",
            for_span,
        )?;
        let Operation::MapIterationFirst { source } = first.operation else {
            unreachable!()
        };
        let birth = g.definition(EntityId::Loan(source))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("old source lease", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            matches!(root.operation,Operation::RootPlace{owner} if EntityId::Value(owner)==a),
            "For borrows original make result, not new installation",
            root,
        )?;
        let Operation::MapIterationHasNext {
            source: hs,
            cursor: hc,
        } = has.operation
        else {
            unreachable!()
        };
        let Operation::MapIterationKeyLoan {
            source: bs,
            cursor: bc,
        } = key.operation
        else {
            unreachable!()
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(has.block)?.kind
        else {
            return Err(failure("old provider actual guard", has));
        };
        need(
            *condition == value(has)?
                && when_true.target == key.block
                && val.block == key.block
                && val.operation
                    == Operation::MapIterationValueLoan {
                        source: bs,
                        cursor: bc,
                    },
            "old actual guard and component identities",
            (has, key, val),
        )?;
        let entry = g.branch(first.block)?;
        let back = g.branch(key.block)?;
        let ho = g.transported(entry, a)?;
        let bo = g.transported(when_true, ho)?;
        let xo = g.transported(when_false, ho)?;
        g.incoming_from(first.block, &[])?;
        g.incoming_from(has.block, &[first.block, key.block])?;
        g.incoming_from(key.block, &[has.block])?;
        g.incoming_from(when_false.target, &[has.block])?;
        need(
            entry.target == has.block
                && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
                && g.transported(entry, result(first)?)? == EntityId::Value(hc)
                && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc)
                && back.target == has.block
                && g.argument(back, ho)? == bo
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.argument(back, EntityId::Value(hc))? == result(next)?
                && next.operation
                    == Operation::MapIterationNext {
                        source: bs,
                        cursor: bc,
                    },
            "every old owner/source/cursor incoming uses exact current columns",
            (entry, back),
        )?;
        let xs = g.transported(when_false, EntityId::Loan(hs))?;
        let xc = g.transported(when_false, EntityId::Value(hc))?;
        let end = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==xs),
            ),
            "old exact source End",
        )?;
        need(
            end.block == when_false.target,
            "old End on actual exhaustion",
            end,
        )?;
        let drops=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop{owner} if f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==map_type)).collect::<Vec<_>>();
        need(
            drops.len() == 2,
            "only actual old and new epoch Drop sites",
            &drops,
        )?;
        let old_drop = one(
            drops.iter().copied().filter(
                |i| matches!(i.operation,Operation::Drop{owner} if EntityId::Value(owner)==xo),
            ),
            "old Drop exact current terminal owner",
        )?;
        let new_drop = one(
            drops.iter().copied().filter(
                |i| matches!(i.operation,Operation::Drop{owner} if EntityId::Value(owner)==n),
            ),
            "new Drop exact installed other result",
        )?;
        need(
            old_drop.block == end.block
                && g.position(old_drop)? == g.position(end)? + 1
                && old_drop.origin == Origin::Source(s.binding),
            "old actual ASAP Drop, not conditional RHS remnant",
            old_drop,
        )?;
        need(
            new_drop.block == new.block
                && g.position(new_drop)? == g.position(new)? + 1
                && new_drop.origin
                    == Origin::Source(if unit_mode {
                        span(parsed, s.assignment)
                    } else {
                        s.binding
                    }),
            "new complete installation ASAP Drop has its own event origin",
            new_drop,
        )?;
        need(
            new.block == old_drop.block && g.position(old_drop)? < g.position(new)?,
            "old actual cleanup precedes later producer",
            (old_drop, new),
        )?;
        let tail_span = one(
            parsed.ast().expressions().iter().filter_map(|(_, i)| {
                let Expression::Call { arguments, .. } = i.payload() else {
                    return None;
                };
                let [arg] = arguments.as_slice() else {
                    return None;
                };
                (inside(i.span(), s.scan)
                    && sources.slice(span(parsed, arg.value)) == Ok("\"tail\""))
                .then_some(i.span())
            }),
            "actual tail marker AST",
        )?;
        let tail = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(tail_span)
                    && matches!(
                        i.operation,
                        Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                    )
            }),
            "actual tail marker",
        )?;
        need(
            tail.block == new_drop.block && g.position(new_drop)? < g.position(tail)?,
            "new epoch dropped before tail",
            tail,
        )?;
        let mut uses = Uses::default();
        for e in [
            a,
            ho,
            bo,
            xo,
            n,
            EntityId::Place(place),
            EntityId::Loan(source),
            EntityId::Loan(hs),
            EntityId::Loan(bs),
            xs,
            result(first)?,
            EntityId::Value(hc),
            EntityId::Value(bc),
            xc,
            result(next)?,
            result(key)?,
            result(val)?,
        ] {
            uses.protect(e);
        }
        uses.permit(a, root);
        uses.permit(EntityId::Place(place), birth);
        uses.permit(EntityId::Loan(source), first);
        uses.permit(EntityId::Loan(hs), has);
        uses.permit(EntityId::Value(hc), has);
        for i in [key, val, next] {
            uses.permit(EntityId::Loan(bs), i);
            uses.permit(EntityId::Value(bc), i);
        }
        uses.permit(xs, end);
        uses.permit(xo, old_drop);
        uses.permit(n, new_drop);
        let read=one(g.instructions.iter().copied().filter(|i|i.block==key.block&&matches!(&i.operation,Operation::DirectCall{receiver:None,arguments,..} if arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])) ,"old component reader exact operands")?;
        let read_decl=one(parsed.roots().iter().filter_map(|id|{let i=parsed.ast().items().get(*id).ok()?;matches!(i.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("read")).then_some(i.span())}),"original component read declaration")?;
        let reader = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(read_decl)),
            "actual selected component reader",
        )?;
        need(
            matches!(read.operation,Operation::DirectCall{callee,..} if callee==reader.id),
            "read call selects original source declaration",
            read,
        )?;
        for (slot, (component, target)) in
            [(key, key_type), (val, value_type)].into_iter().enumerate()
        {
            let expected = EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            };
            need(
                f.entity(result(component)?)
                    .is_some_and(|d| d.ty == expected)
                    && reader
                        .block(reader.entry_block().unwrap())
                        .unwrap()
                        .parameters
                        .get(slot)
                        .and_then(|p| reader.entity(*p))
                        .is_some_and(|d| d.ty == expected),
                "component and selected reader preserve actual Shared String ABI",
                component,
            )?;
        }
        let mut ends = Vec::new();
        for component in [key, val] {
            let e = result(component)?;
            let ending = one(
                g.instructions.iter().copied().filter(|i| {
                    i.operation
                        == Operation::BorrowEnd {
                            loan: loan(component).unwrap(),
                        }
                }),
                "actual old component End",
            )?;
            need(ending.block == key.block, "old body component End", ending)?;
            uses.permit(e, read);
            uses.permit(e, ending);
            ends.push(ending);
        }
        need(
            g.position(read)? < g.position(ends[1])?
                && g.position(ends[1])? < g.position(ends[0])?
                && g.position(ends[0])? < g.position(next)?,
            "old read/VEnd/KEnd/Next order",
            &ends,
        )?;
        let body_span = parsed.ast().statements().get(s.body).unwrap().span();
        let (guard_name, guard_call) = one(
            parsed
                .ast()
                .items()
                .iter()
                .filter_map(|(_, i)| match i.payload() {
                    Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        ..
                    } if inside(i.span(), body_span) && sources.slice(*name) == Ok("local") => {
                        Some((*name, *initializer))
                    }
                    _ => None,
                }),
            "old actual Guard local",
        )?;
        let guard = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(span(parsed, guard_call))
                    && matches!(i.operation, Operation::HeapAllocate { .. })
            }),
            "old actual Guard allocation",
        )?;
        let guard_owner = value(guard)?;
        let guard_drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner: guard_owner }),
            "old actual Guard Drop",
        )?;
        need(
            guard.block == read.block
                && guard_drop.block == read.block
                && guard_drop.origin == Origin::Source(guard_name)
                && g.position(guard)? < g.position(read)?
                && g.position(read)? < g.position(guard_drop)?
                && g.position(guard_drop)? < g.position(ends[1])?,
            "Guard is dropped after read before VEnd",
            guard_drop,
        )?;
        uses.protect(EntityId::Value(guard_owner));
        uses.permit(EntityId::Value(guard_owner), guard_drop);
        uses.check(&g, None)?;
        let cleanup = old_drop.block;
        need(
            matches!(&g.term(cleanup)?.kind,TerminatorKind::Return{values} if values.is_empty())
                && g.edges
                    .iter()
                    .all(|(from, e)| !(*from == cleanup && e.target == has.block)),
            "both epoch Drops lie outside loop and lead to Unit Return",
            cleanup,
        )?;
        need(
            g.blocks
                .iter()
                .filter(|&&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. }))
                .count()
                == 1,
            "every finite path has one common old/new cleanup and Unit Return",
            f.id,
        )?;
    }
    Ok(())
}
#[test]
fn completed_assignment_event_keeps_its_own_epoch() {
    let mut results = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observed(&case, unit);
            println!(
                "COMPLETED EPOCH CASE {} unit={unit} result={result:?}",
                case.label
            );
            results.push((case.label.clone(), unit, result));
        }
    }
    assert_eq!(results.len(), 8);
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}
