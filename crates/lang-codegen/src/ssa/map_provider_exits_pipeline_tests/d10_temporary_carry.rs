//! 原 w09 字节的 D10 普通成功合同。两臂 fresh；先保全真实阶段错误，再统一断言。
//! 本首红只锁原 AST/FE/plan、返回 main 与 factory/provider birth；不声称 carry 已独立验收。
use super::{
    Report,
    fixture::{Failure, clean, contains, failure, need, verify_failure},
    identity, pipeline,
    query_graph::{Graph, loan, result, value},
    query_inputs::one,
    query_temporary_cleanup,
};
use crate::ssa::model::{EntityId, FunctionId, LoanKind, Operation, Origin, Program};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    lexer::lex,
    name_resolution::{NameResolution, SymbolKind, ValidatedCompilationUnitNames},
    ownership_checking as ownership,
    parser::{
        AssignmentOperator, Expression, ForBinding, Item, NameMarker, ParsedFile, Statement,
        parse_file,
    },
    source::{SourceMap, Span},
    type_checking as types,
};

const PATH: &str = "w09_map_temp_source.ko";
const SOURCE: &str =
    include_str!("../../../tests/fixtures/d10-unit-migration-0341/w09_map_temp_source.ko");
const SHA256: &str = "55ce4c7d25c06516cfacd9921fcd16b90c15c63adbd9d44d9133c92c49539007";

#[derive(Debug)]
struct Anchors {
    main: Span,
    make: Span,
    make_name: Span,
    statement: StatementId,
    body: StatementId,
    source: ExpressionId,
    source_span: Span,
    bindings: [Span; 2],
    assignment: ExpressionId,
    target: ExpressionId,
    rhs: ExpressionId,
}

fn function(
    sources: &SourceMap,
    parsed: &ParsedFile,
    expected: &str,
) -> Result<(Span, Span), Failure> {
    let mut found = Vec::new();
    for &root in parsed.roots() {
        let node = parsed
            .ast()
            .items()
            .get(root)
            .map_err(|e| failure("w09 function AST", e))?;
        if let Item::Function {
            name: NameMarker::Present(name),
            ..
        } = node.payload()
            && sources.slice(*name) == Ok(expected)
        {
            found.push((node.span(), *name));
        }
    }
    one(found.into_iter(), "w09 unique original function")
}

fn anchors(sources: &SourceMap, parsed: &ParsedFile) -> Result<Anchors, Failure> {
    let (main, _) = function(sources, parsed, "main")?;
    let (make, make_name) = function(sources, parsed, "make")?;
    let (statement, node) = one(
        parsed
            .ast()
            .statements()
            .iter()
            .filter(|(_, n)| matches!(n.payload(), Statement::For { .. })),
        "w09 sole actual For",
    )?;
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        source,
        body,
        ..
    } = node.payload()
    else {
        return Err(failure("w09 destructuring For", node));
    };
    let [NameMarker::Present(key), NameMarker::Present(value_marker)] = names.as_slice() else {
        return Err(failure("w09 two actual component markers", names));
    };
    let source_span = parsed
        .ast()
        .expressions()
        .get(*source)
        .map_err(|e| failure("w09 source AST", e))?
        .span();
    let body_span = parsed
        .ast()
        .statements()
        .get(*body)
        .map_err(|e| failure("w09 body AST", e))?
        .span();
    let (assignment, assigned) = one(
        parsed.ast().expressions().iter().filter(|(_, n)| {
            contains(body_span, n.span()) && matches!(n.payload(), Expression::Assignment { .. })
        }),
        "w09 sole body assignment",
    )?;
    let Expression::Assignment {
        target,
        operator: AssignmentOperator::Assign,
        value,
        ..
    } = assigned.payload()
    else {
        return Err(failure("w09 ordinary assignment AST", assigned));
    };
    need(
        parsed.package().is_none()
            && parsed.roots().len() == 2
            && contains(main, node.span())
            && sources.source_name(parsed.source_id()) == Ok(PATH)
            && sources.slice(source_span) == Ok("make()"),
        "w09 original default-package source and main For",
        (main, node, source_span),
    )?;
    Ok(Anchors {
        main,
        make,
        make_name,
        statement,
        body: *body,
        source: *source,
        source_span,
        bindings: [*key, *value_marker],
        assignment,
        target: *target,
        rhs: *value,
    })
}

fn single_facts(
    a: &Anchors,
    names: &NameResolution,
    typed: &types::TypedFile,
    owned: &ownership::OwnershipCheckedFile,
) -> Result<(), Failure> {
    use ownership::{
        DropPoint as P, DropTarget as T, IterationCleanupAction as A, IterationExitKind as E,
    };
    let d = typed
        .map_iteration(a.statement)
        .ok_or_else(|| failure("w09 single descriptor", a))?;
    let p = owned
        .map_iteration(a.statement)
        .ok_or_else(|| failure("w09 single plan", a))?;
    println!("D10 W09 SINGLE REAL descriptor={d:#?} plan={p:#?}");
    let make = one(
        names.symbols().iter().filter(|s| {
            s.kind() == SymbolKind::Function && s.span() == a.make_name && !s.is_synthetic()
        }),
        "w09 actual single make symbol",
    )?;
    let call = typed
        .call(a.source)
        .ok_or_else(|| failure("w09 single source Call", a))?;
    need(
        d.source() == a.source
            && p.descriptor() == d
            && typed.expression_category(a.source) == Some(types::ExpressionCategory::Temporary)
            && call.target() == types::CallableTarget::Source(make.id())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == d.source_type()
            && call.result_source() == types::CallableResultSource::Owned
            && p.source_access() == ownership::MapIterationSourceAccess::Temporary
            && p.source_kind()
                == &ownership::IterationSource::BorrowedSource(ownership::LoanTarget::Temporary(
                    a.source,
                ))
            && typed.types().get(d.key_type())
                == Some(&types::TypeKind::Builtin(types::BuiltinType::String))
            && typed.types().get(d.value_type())
                == Some(&types::TypeKind::Builtin(types::BuiltinType::Int)),
        "w09 single actual Call/Temporary/source types",
        (d, p, call),
    )?;
    for (i, component) in [
        ownership::MapIterationComponent::Key,
        ownership::MapIterationComponent::Value,
    ]
    .into_iter()
    .enumerate()
    {
        let symbol = d.bindings()[i];
        let loan = &p.loans()[i];
        need(
            names.symbols().iter().any(|s| {
                s.id() == symbol && s.span() == a.bindings[i] && s.kind() == SymbolKind::ForBinding
            }) && d.bindings()[0] != d.bindings()[1]
                && loan.binding() == symbol
                && loan.component() == component
                && loan.statement() == a.statement
                && loan.source() == a.source
                && Some(loan.origin()) == p.source()
                && p.bindings()[i].symbol() == symbol
                && p.bindings()[i].kind() == ownership::OwnershipBindingKind::Shared
                && owned.binding_kind(symbol) == Some(ownership::OwnershipBindingKind::Shared),
            "w09 single component identity",
            (symbol, loan),
        )?;
    }
    let drop = one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == T::Temporary(a.source)),
        "w09 single unique source Drop",
    )?;
    need(
        drop.point() == P::LoopExit(a.statement) && drop.value_origin() == a.source_span,
        "w09 single Drop exact source/point",
        drop,
    )?;
    let expected = [
        (
            E::Fallthrough,
            P::AfterStatement(a.body),
            vec![
                A::EndBinding {
                    statement: a.statement,
                    symbol: d.value_binding(),
                },
                A::EndBinding {
                    statement: a.statement,
                    symbol: d.key_binding(),
                },
            ],
        ),
        (
            E::Exhaustion,
            P::LoopExit(a.statement),
            vec![
                A::FinishProvider(a.statement),
                A::EndSource(a.statement),
                A::Drop(*drop),
            ],
        ),
    ];
    need(
        p.exits().len() == expected.len(),
        "w09 single complete exit set",
        p.exits(),
    )?;
    for (kind, point, actions) in expected {
        let exit = one(
            p.exits().iter().filter(|e| e.kind() == kind),
            "w09 single unique exit",
        )?;
        let complete = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, action)| (*at == point).then_some(*action))
            .collect::<Vec<_>>();
        need(
            exit.point() == point
                && exit.actions() == actions
                && complete == actions
                && owned.cleanup_conditions().get(exit.condition())
                    == Some(&ownership::CleanupCondition::Always),
            "w09 single complete ordered cleanup",
            (exit, complete, actions),
        )?;
    }
    Ok(())
}

fn unit_facts(
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &types::CompilationUnitTypes,
    owned: &ownership::CompilationUnitOwnership,
) -> Result<(), Failure> {
    let source_names = one(
        names.names().source_units().iter(),
        "w09 one original source unit",
    )?;
    let unit = source_names.source_unit();
    let statement = types::UnitStatementId::new(unit, a.statement);
    let expression = |id| types::UnitExpressionId::new(unit, id);
    let source = expression(a.source);
    let d = typed
        .map_iteration(statement)
        .ok_or_else(|| failure("w09 unit descriptor", statement))?;
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("w09 unit plan", statement))?;
    println!("D10 W09 UNIT REAL descriptor={d:#?} plan={p:#?}");
    let make = one(
        names.names().index().declarations().iter().filter(|d| {
            d.source_unit() == unit
                && d.kind() == SymbolKind::Function
                && d.name_span() == a.make_name
        }),
        "w09 actual unit make declaration",
    )?;
    let call = typed
        .call(source)
        .ok_or_else(|| failure("w09 unit source Call", source))?;
    let assignment = typed
        .assignment(expression(a.assignment))
        .ok_or_else(|| failure("w09 unit real assignment", a.assignment))?;
    need(
        d.source() == source
            && p.descriptor() == d
            && typed.expression_category(source) == Some(types::ExpressionCategory::Temporary)
            && call.target() == types::UnitCallTarget::Declaration(make.id())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == d.source_type()
            && call.result_source() == types::CallableResultSource::Owned
            && p.source_access() == ownership::UnitIterationSourceAccess::Temporary
            && p.source_kind()
                == &ownership::IterationSource::BorrowedSource(
                    ownership::UnitLoanTarget::Temporary(source),
                )
            && typed.types().get(d.key_type())
                == Some(&types::UnitTypeKind::Builtin(types::BuiltinType::String))
            && typed.types().get(d.value_type())
                == Some(&types::UnitTypeKind::Builtin(types::BuiltinType::Int))
            && assignment.expression() == expression(a.assignment)
            && assignment.target() == expression(a.target)
            && assignment.value() == expression(a.rhs)
            && assignment.operator() == AssignmentOperator::Assign
            && assignment.target_type() == d.value_type()
            && assignment.falls_through(),
        "w09 unit actual Call/Temporary/assignment facts",
        (d, p, call, assignment),
    )?;
    for (i, component) in [
        ownership::MapIterationComponent::Key,
        ownership::MapIterationComponent::Value,
    ]
    .into_iter()
    .enumerate()
    {
        let symbol = d.bindings()[i];
        let loan = &p.loans()[i];
        need(
            symbol.source_unit() == unit
                && source_names.resolution().symbols().iter().any(|s| {
                    s.id() == symbol.symbol()
                        && s.span() == a.bindings[i]
                        && s.kind() == SymbolKind::ForBinding
                })
                && d.bindings()[0] != d.bindings()[1]
                && loan.binding() == symbol
                && loan.component() == component
                && loan.statement() == statement
                && loan.source() == source
                && Some(loan.origin()) == p.source()
                && p.bindings()[i].symbol() == symbol
                && p.bindings()[i].kind() == ownership::OwnershipBindingKind::Shared
                && owned.binding_kind(symbol) == Some(ownership::OwnershipBindingKind::Shared),
            "w09 unit component identity",
            (symbol, loan),
        )?;
    }
    query_temporary_cleanup::unit(
        types::UnitStatementId::new(unit, a.body),
        a.source_span,
        statement,
        source,
        owned,
    )
}

fn source_birth(
    sources: &SourceMap,
    a: &Anchors,
    program: &Program,
    entry: FunctionId,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "w09 one module")?;
    let main = module
        .function(entry)
        .ok_or_else(|| failure("w09 returned entry", entry))?;
    need(
        main.origin == Origin::Source(a.main),
        "w09 returned main source identity",
        &main.origin,
    )?;
    identity::entry_sources(
        sources,
        program,
        entry,
        &[("main", a.main), ("make", a.make)],
    )?;
    let make = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(a.make)),
        "w09 real make function",
    )?;
    let graph = Graph::with_abort(main)?;
    let call = one(
        graph.instructions.iter().copied().filter(
            |i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == make.id),
        ),
        "w09 unique reachable factory Call",
    )?;
    need(
        call.origin == Origin::Source(a.source_span)
            && matches!(&call.operation, Operation::DirectCall { receiver: None, arguments, .. } if arguments.is_empty()),
        "w09 actual source Call birth",
        call,
    )?;
    let owner = value(call)?;
    let root = one(
        graph
            .instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::RootPlace { owner }),
        "w09 actual hidden-owner RootPlace",
    )?;
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("w09 hidden-owner place", root));
    };
    let borrow = one(
        graph.instructions.iter().copied().filter(|i| {
            i.operation
                == Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                }
        }),
        "w09 hidden-owner source loan",
    )?;
    let source = loan(borrow)?;
    let first = one(
        graph
            .instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::MapIterationFirst { source }),
        "w09 actual provider First",
    )?;
    need(
        [root, borrow, first].iter().all(|i| i.block == call.block)
            && graph.position(call)? < graph.position(root)?
            && graph.position(root)? < graph.position(borrow)?
            && graph.position(borrow)? < graph.position(first)?,
        "w09 Call precedes actual provider acquisition",
        (call, root, borrow, first),
    )
}

#[derive(Debug)]
struct Observation {
    unit: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn observe(unit: bool) -> Observation {
    let mut report = Report::default();
    // Hash is the separately checked frozen identity; no runtime hash/native claim.
    println!(
        "D10 W09 INPUT unit={unit} path={PATH} bytes={} frozen_sha256={SHA256}",
        SOURCE.len()
    );
    let outcome = (|| {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(PATH, SOURCE)
            .map_err(|e| failure("w09 source", e))?;
        need(
            SOURCE.len() == 222,
            "w09 original source byte length",
            SOURCE.len(),
        )?;
        let lexed = lex(&sources, source).map_err(|e| failure("w09 lex infrastructure", e))?;
        clean(&sources, "w09 lex diagnostics", lexed.diagnostics())?;
        let parsed =
            parse_file(&sources, &lexed).map_err(|e| failure("w09 parse infrastructure", e))?;
        clean(&sources, "w09 parse diagnostics", parsed.diagnostics())?;
        let anchors = anchors(&sources, &parsed);
        report.query("w09 original AST anchors", &anchors);
        let (program, entry) = if unit {
            pipeline::unit_observed_with_entry(
                &sources,
                &parsed,
                PATH,
                "main",
                &mut report,
                |n, t, o, r| {
                    let facts = anchors
                        .as_ref()
                        .map_err(|e| failure("w09 AST for unit query", e))
                        .and_then(|a| unit_facts(a, n, t, o));
                    r.query("w09 same-analysis unit source and complete cleanup", &facts);
                },
            )
        } else {
            pipeline::single_observed_with_entry(
                &sources,
                &parsed,
                "main",
                &mut report,
                |n, t, o, r| {
                    let facts = anchors
                        .as_ref()
                        .map_err(|e| failure("w09 AST for single query", e))
                        .and_then(|a| single_facts(a, n, t, o));
                    r.query(
                        "w09 same-analysis single source and complete cleanup",
                        &facts,
                    );
                },
            )
        }?;
        let birth = anchors
            .as_ref()
            .map_err(|e| failure("w09 AST for SSA query", e))
            .and_then(|a| source_birth(&sources, a, &program, entry));
        report.query(
            "w09 returned main and actual factory/provider birth",
            &birth,
        );
        // Read-only query failures never replace the original verifier/LLVM error.
        crate::ssa::verify::verify_program(&program).map_err(|e| verify_failure(&sources, e))?;
        report.check("w09 original SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("w09 LLVM render/verify", e))?;
        report.check("w09 original LLVM render/verify", true, llvm.len());
        Ok(())
    })();
    Observation {
        unit,
        report,
        outcome,
    }
}

#[test]
fn w09_temporary_map_carry_reaches_verified_llvm() {
    let observations = [observe(false), observe(true)];
    for observation in &observations {
        println!(
            "D10 W09 OUTCOME unit={} report={:#?} outcome={:?}",
            observation.unit, observation.report, observation.outcome
        );
    }
    assert!(
        observations
            .iter()
            .all(|o| o.report.passed() && o.outcome.is_ok()),
        "原222B w09 的 single/unit 两 fresh 必须完成同轮完整 FE/sealed、真实 main、SSA 与 LLVM；UnsupportedNode 不是成功 oracle。\n{observations:#?}"
    );
}
