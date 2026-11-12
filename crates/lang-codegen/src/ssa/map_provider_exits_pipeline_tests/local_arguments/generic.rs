//! Concrete Map return with an unused generic parameter exercises the ordinary generic ABI.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
use super::temporary_arguments::{ast, facts};
mod graph {
    //! Selected argument evaluation, ordinary Borrow ABI and the actual returned owner.

    use super::{Case, ast};
    use crate::ssa::map_provider_exits_pipeline_tests::temporary_arguments::graph::provider;
    use crate::ssa::{
        map_provider_exits_pipeline_tests::{
            fixture::{Failure, failure, need},
            query_graph::{Graph, Uses, function, value},
            query_inputs::one,
        },
        model::*,
    };
    use lang_frontend::{
        parser::ParsedFile,
        source::{SourceMap, Span},
    };

    fn at<'ir>(
        g: &Graph<'ir>,
        origin: Span,
        predicate: impl Fn(&Operation) -> bool,
    ) -> Result<&'ir Instruction, Failure> {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(origin) && predicate(&i.operation)),
            "argument actual source-origin SSA operation",
        )
    }

    fn marker<'ir>(g: &Graph<'ir>, span: Span) -> Result<&'ir Instruction, Failure> {
        at(g, span, |operation| match operation {
            Operation::PrintLiteral { bytes } => bytes == b"after\n",
            Operation::PrintString { value } => {
                let Ok(begin) = g.definition(EntityId::Loan(*value)) else {
                    return false;
                };
                let Operation::BorrowBegin { place, .. } = begin.operation else {
                    return false;
                };
                let Ok(root) = g.definition(EntityId::Place(place)) else {
                    return false;
                };
                let Operation::RootPlace { owner } = root.operation else {
                    return false;
                };
                g.definition(EntityId::Value(owner)).is_ok_and(|literal|
                    matches!(&literal.operation, Operation::StringLiteral {bytes, ..} if bytes == b"after"))
            }
            _ => false,
        })
    }

    pub(super) fn check(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        program: &Program,
        control: bool,
    ) -> Result<(), Failure> {
        let input = ast::input(sources, parsed, control)?;
        let module = one(program.modules.iter(), "argument actual original module")?;
        let entry = function(module, input.entry.span)?;
        let factory = function(module, input.factory.span)?;
        let argument = function(module, input.argument.span)?;
        let g = Graph::with_abort(entry)?;
        let block = entry.entry_block().unwrap();
        need(
            g.block(block)?.parameters.is_empty(),
            "argument caller entry has no parameters",
            entry.id,
        )?;
        let value_call = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation, Operation::DirectCall {callee, ..} if callee == argument.id),
            ),
            "argument evaluator is actually called once",
        )?;
        need(
            value_call.origin == Origin::Source(ast::span(parsed, input.value))
                && matches!(&value_call.operation, Operation::DirectCall {receiver:None, arguments, ..} if arguments.is_empty()),
            "argument evaluator selected actual source Call",
            value_call,
        )?;
        let actual_value = value(value_call)?;
        let int = entry
            .entity(EntityId::Value(actual_value))
            .unwrap()
            .ty
            .semantic_type();
        need(
            module.type_kind(int)
                == Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
                && argument.return_types.as_slice() == [int]
                && argument
                    .block(argument.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .is_empty(),
            "argument evaluator actual Int return ABI",
            argument.id,
        )?;
        let call = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation, Operation::DirectCall {callee, ..} if callee == factory.id),
            ),
            "argument selected Int factory called once",
        )?;
        need(
            call.origin == Origin::Source(ast::span(parsed, input.produce)),
            "argument actual selected Int overload Callspan",
            call,
        )?;
        let Operation::DirectCall {
            receiver: None,
            ref arguments,
            ..
        } = call.operation
        else {
            return Err(failure("argument selected direct factory ABI", call));
        };
        let [EntityId::Loan(actual_loan)] = arguments.as_slice() else {
            return Err(failure("argument actual SharedBorrow Int operand", call));
        };
        let begin = g.definition(EntityId::Loan(*actual_loan))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure("argument actual shared loan birth", begin));
        };
        let root = g.definition(EntityId::Place(place))?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *actual_loan }),
            "argument actual unique synchronous Borrow End",
        )?;
        let parameters = &factory
            .block(factory.entry_block().unwrap())
            .unwrap()
            .parameters;
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target: int,
        };
        need(
            root.operation
                == Operation::RootPlace {
                    owner: actual_value,
                }
                && root.origin == Origin::Source(ast::span(parsed, input.value))
                && begin.origin == root.origin
                && end.origin == Origin::Source(ast::span(parsed, input.produce))
                && [value_call, root, begin, call, end]
                    .iter()
                    .all(|i| i.block == block)
                && g.position(value_call)? < g.position(root)?
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(call)?
                && g.position(call)? < g.position(end)?
                && entry.entity(EntityId::Loan(*actual_loan)).unwrap().ty == ty
                && entry.entity(EntityId::Place(place)).unwrap().ty == EntityType::Place(int)
                && matches!(parameters.as_slice(), [parameter] if factory.entity(*parameter).unwrap().ty == ty),
            "argument actual evaluator Value to Root/SharedBorrow/callee ABI/End order",
            (value_call, root, begin, call, end),
        )?;
        let mut uses = Uses::default();
        uses.protect(EntityId::Value(actual_value));
        uses.permit(EntityId::Value(actual_value), root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        uses.protect(EntityId::Loan(*actual_loan));
        uses.permit(EntityId::Loan(*actual_loan), call);
        uses.permit(EntityId::Loan(*actual_loan), end);
        let owner = value(call)?;
        let map_type = entry
            .entity(EntityId::Value(owner))
            .unwrap()
            .ty
            .semantic_type();
        let (kind, key, val) = module
            .map_container(map_type)
            .ok_or_else(|| failure("argument actual owning factory result", call))?;
        need(
            kind == if case.mutable {
                MapContainerKind::MutableMap
            } else {
                MapContainerKind::Map
            } && [key, val]
                .iter()
                .all(|ty| module.type_kind(*ty) == Some(&SsaTypeKind::StringOwner))
                && factory.return_types.as_slice() == [map_type]
                && g.instructions
                    .iter()
                    .filter(|i| {
                        matches!(i.operation, Operation::DirectCall { .. })
                            && i.results.iter().any(|entity| {
                                module
                                    .map_container(entry.entity(*entity).unwrap().ty.semantic_type())
                                    .is_some()
                            })
                    })
                    .count()
                    == 1,
            "argument exact selected map result and no alternate factory result",
            call,
        )?;
        super::wrapper::ssa(case.mutable, sources, parsed, module, factory, map_type)?;
        let after = marker(&g, input.after)?;
        if let Some(site) = &input.loop_site {
            provider::check(
                &g,
                module,
                function(module, input.reader.span)?,
                parsed,
                site,
                (owner, call, end, after),
                &mut uses,
            )?;
        } else {
            uses.protect(EntityId::Value(owner));
            let size = at(&g, input.size.unwrap(), |o| {
                matches!(o, Operation::MapSize { .. })
            })?;
            need(
                size.operation
                    == Operation::MapSize {
                        owner: EntityId::Value(owner),
                    }
                    && size.block == block
                    && g.position(end)? < g.position(size)?,
                "argument control queries its actual returned owner after argument End",
                size,
            )?;
            uses.permit(EntityId::Value(owner), size);
            let drop = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "argument control actual unique returned-owner Drop",
            )?;
            need(
                drop.block == block && g.position(drop)? == g.position(size)? + 1,
                "argument control Pure owner drops after actual last size",
                drop,
            )?;
            uses.permit(EntityId::Value(owner), drop);
            need(
                !g.instructions.iter().any(|i| {
                    matches!(
                        i.operation,
                        Operation::MapIterationFirst { .. }
                            | Operation::MapIterationHasNext { .. }
                            | Operation::MapIterationNext { .. }
                            | Operation::MapIterationKeyLoan { .. }
                            | Operation::MapIterationValueLoan { .. }
                    )
                }),
                "argument no-For control contains no provider operations",
                entry.id,
            )?;
        }
        need(g.instructions.iter().filter(|i| matches!(i.operation, Operation::Drop {owner}
            if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count() == 1,
            "argument caller drops only its actual returned Map", entry.id)?;
        uses.check(&g, None)
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms, temporary_arguments,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};
use temporary_arguments::Case;
pub(super) mod wrapper {
    //! A generic forwarding Call returns the exact owner built by the existing ordinary implementation.
    use super::ast;
    use crate::ssa::map_provider_exits_pipeline_tests::temporary_arguments::graph::factory;
    use crate::ssa::map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function, result, value},
        query_inputs::one,
    };
    use crate::ssa::model::*;
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::{UnitReferenceTarget, ValidatedCompilationUnitNames},
        ownership_checking::{CompilationUnitOwnership, LoanKind as FeLoanKind, UnitLoanTarget},
        parser::{Expression, ParsedFile},
        source::{SourceMap, Span},
        type_checking::*,
    };
    struct Input {
        wrapper: ast::Function,
        implementation: ast::Function,
        call: ExpressionId,
        argument: ExpressionId,
        returned: Span,
    }
    fn input(sources: &SourceMap, parsed: &ParsedFile) -> Result<Input, Failure> {
        let wrapper = ast::function(sources, parsed, "produce", Some("Int"))?;
        let implementation = ast::function(sources, parsed, "nonGenericImpl", Some("Int"))?;
        let call = one(
            parsed.ast().expressions().iter().filter_map(|(id, n)| {
                let Expression::Call {
                    callee, arguments, ..
                } = n.payload()
                else {
                    return None;
                };
                let [argument] = arguments.as_slice() else {
                    return None;
                };
                (ast::inside(n.span(), wrapper.span)
                    && sources.slice(ast::span(parsed, *callee)) == Ok("nonGenericImpl"))
                .then_some((id, argument.value))
            }),
            "generic wrapper actual ordinary implementation Call",
        )?;
        need(
            sources.slice(ast::span(parsed, call.1)) == Ok("input")
                && matches!(
                    parsed.ast().expressions().get(call.1).unwrap().payload(),
                    Expression::Name
                ),
            "generic wrapper forwards original input Name",
            call,
        )?;
        let returned = one(
            parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Expression::Return {
                        value: Some(value), ..
                    } if ast::inside(n.span(), wrapper.span) && *value == call.0 => Some(n.span()),
                    _ => None,
                }),
            "generic wrapper exact Return of implementation Call",
        )?;
        Ok(Input {
            wrapper,
            implementation,
            call: call.0,
            argument: call.1,
            returned,
        })
    }

    pub(in super::super) fn unit(
        sources: &SourceMap,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        let i = input(sources, parsed)?;
        let resolution = one(
            names.names().source_units().iter(),
            "generic wrapper original unit",
        )?;
        let unit = resolution.source_unit();
        let e = |id| UnitExpressionId::new(unit, id);
        let declaration = one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == unit && d.name_span() == i.implementation.name),
            "generic wrapper actual implementation declaration",
        )?;
        let wrapper = one(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|d| d.callable())
                .filter(|s| s.name_span() == i.wrapper.name),
            "generic wrapper actual source signature",
        )?;
        let parameter = wrapper.parameters()[0]
            .symbol()
            .ok_or_else(|| failure("generic wrapper original parameter symbol", wrapper))?;
        let call = typed
            .call(e(i.call))
            .ok_or_else(|| failure("generic wrapper qualified inner Call", i.call))?;
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, i.argument)),
            "generic wrapper qualified actual input Name",
        )?;
        let signature = typed
            .signatures()
            .declaration(declaration.id())
            .and_then(|d| d.callable())
            .ok_or_else(|| failure("generic wrapper actual ordinary signature", declaration))?;
        need(
            call.target() == UnitCallTarget::Declaration(declaration.id())
                && call.receiver().is_none()
                && call.instance().type_arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && call.return_type() == signature.return_type()
                && matches!(call.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.category()==ExpressionCategory::Place&&a.parameter_type()==signature.parameters()[0].ty())
                && reference.target() == &UnitReferenceTarget::Symbol(parameter),
            "generic wrapper qualified actual ordinary target and original Borrow forwarding",
            call,
        )?;
        let contract = one(
            owned.call_argument_contracts_for(e(i.call)),
            "generic wrapper actual input ownership contract",
        )?;
        let loan = one(
            owned
                .loans()
                .iter()
                .filter(|l| l.call() == e(i.call) && l.argument() == e(i.argument)),
            "generic wrapper original qualified loan",
        )?;
        need(
            contract.argument() == e(i.argument)
                && contract.parameter_index() == 0
                && contract.parameter_span() == Some(signature.parameters()[0].span())
                && contract.category() == ExpressionCategory::Place
                && contract.kind()
                    == lang_frontend::ownership_checking::UnitCallArgumentOwnershipKind::SharedLoan
                && loan.kind() == FeLoanKind::Shared
                && matches!(loan.target(),UnitLoanTarget::Place(p)if p.is_root()&&p.root()==parameter)
                && loan.begin_span() == ast::span(parsed, i.argument)
                && loan.end_span() == ast::span(parsed, i.call),
            "generic wrapper exact qualified ownership loan",
            (contract, loan),
        )
    }
    pub(in super::super) fn ssa(
        mutable: bool,
        sources: &SourceMap,
        parsed: &ParsedFile,
        module: &Module,
        wrapper: &Function,
        map: SsaTypeId,
    ) -> Result<(), Failure> {
        let i = input(sources, parsed)?;
        let implementation = function(module, i.implementation.span)?;
        let g = Graph::new(wrapper)?;
        need(
            g.blocks.len() == 1 && g.edges.is_empty(),
            "generic wrapper has one actual forwarding block",
            wrapper.id,
        )?;
        let params = &g.block(wrapper.entry_block().unwrap())?.parameters;
        let parameter = one(params.iter().copied(), "generic wrapper sole Borrow formal")?;
        need(
            matches!(
                wrapper.entity(parameter).unwrap().ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    ..
                }
            ) && implementation
                .block(implementation.entry_block().unwrap())
                .unwrap()
                .parameters
                .len()
                == 1
                && wrapper.entity(parameter).unwrap().ty
                    == implementation
                        .entity(
                            implementation
                                .block(implementation.entry_block().unwrap())
                                .unwrap()
                                .parameters[0],
                        )
                        .unwrap()
                        .ty
                && wrapper.return_types.as_slice() == [map]
                && implementation.return_types.as_slice() == [map],
            "generic wrapper and ordinary impl exact borrowed-Int/owned-Map ABI",
            (wrapper.id, implementation.id),
        )?;
        let call=one(g.instructions.iter().copied().filter(|n|matches!(n.operation,Operation::DirectCall{callee,receiver:None,..}if callee==implementation.id)&&n.origin==Origin::Source(ast::span(parsed,i.call))),"generic wrapper selects actual implementation once")?;
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let actual = one(
            arguments.iter().copied(),
            "generic wrapper sole forwarded ABI argument",
        )?;
        let mut lineage = std::collections::BTreeSet::from([parameter]);
        let mut uses = Uses::default();
        uses.protect(parameter);
        let mut called = false;
        let mut ended = std::collections::BTreeSet::new();
        for op in &g.instructions {
            match op.operation {
                Operation::SharedReborrow { source } => {
                    let parent = EntityId::Loan(source);
                    let child = result(op)?;
                    need(
                        !called
                            && lineage.contains(&parent)
                            && !ended.contains(&parent)
                            && op.origin == Origin::Source(ast::span(parsed, i.argument))
                            && wrapper.entity(parent).unwrap().ty == wrapper.entity(child).unwrap().ty,
                        "generic wrapper exact live source-parameter reborrow",
                        op,
                    )?;
                    uses.permit(parent, op);
                    uses.protect(child);
                    lineage.insert(child);
                }
                Operation::DirectCall { .. } => {
                    need(
                        op.id == call.id
                            && !called
                            && lineage.contains(&actual)
                            && !ended.contains(&actual),
                        "generic wrapper only exact selected forwarding Call",
                        op,
                    )?;
                    uses.permit(actual, op);
                    called = true;
                    uses.protect(result(op)?);
                }
                Operation::BorrowEnd { loan } => {
                    let entity = EntityId::Loan(loan);
                    need(
                        called
                            && lineage.contains(&entity)
                            && ended.insert(entity)
                            && op.origin
                                == Origin::Source(if entity == parameter {
                                    i.returned
                                } else {
                                    ast::span(parsed, i.call)
                                }),
                        "generic wrapper synchronous child or final ABI loan End",
                        op,
                    )?;
                    uses.permit(entity, op);
                }
                _ => {
                    return Err(failure(
                        "generic wrapper unexpected forwarding body operation",
                        op,
                    ));
                }
            }
        }
        let term = g.term(call.block)?;
        need(
            called
                && term.origin == Origin::Source(i.returned)
                && matches!(&term.kind,TerminatorKind::Return{values}if values.as_slice()==[value(call)?])
                && lineage.iter().all(|e| *e == parameter || ended.contains(e)),
            "generic wrapper Return delivers the same implementation owner once",
            term,
        )?;
        uses.check(&g, Some((call.block, value(call)?)))?;
        let case = super::temporary_arguments::sources()
            .into_iter()
            .find(|c| c.mutable == mutable)
            .unwrap();
        factory::check(&case, sources, parsed, module, implementation, map)
    }
}
fn prerequisite_cases() -> Vec<Case> {
    temporary_arguments::sources()
        .into_iter()
        .map(|mut c| {
            let change = |text: String| {
                let declaration = "fun produce(input:Int)";
                assert_eq!(text.matches(declaration).count(), 1);
                let text = text.replace(declaration, "fun <T> produce(input:Int)");
                assert_eq!(text.matches("produce(argument())").count(), 1);
                text.replace("produce(argument())", "produce<Int>(argument())")
            };
            c.label = c
                .label
                .replacen("temporary-argument", "generic-local-argument", 1);
            c.source = change(c.source);
            c.control = change(c.control);
            c
        })
        .collect()
}
// The original direct-body inputs remain a hard-success prerequisite for 0291.
// Actual stable-v2 observations: 9/12 success; all three Single Map shapes stop
// inside generic `result.consume()`, including no-For. No rejection oracle or
// per-kind/per-entry filter registers them as this bounded positive slice.
pub(crate) fn prerequisite_sources() -> Vec<(String, String, String)> {
    tuples(prerequisite_cases())
}
fn cases() -> Vec<Case> {
    temporary_arguments::sources().into_iter().map(|mut c| {
        let kind=if c.mutable {"MutableMap"} else {"Map"};
        let change=|text:String| {
            let declaration="fun produce(input:Int)";
            assert_eq!(text.matches(declaration).count(),1);
            let renamed=text.replace(declaration,"fun nonGenericImpl(input:Int)");
            let wrapper=format!("fun <T> produce(input:Int):{kind}<String,String>{{return nonGenericImpl(input)}}\n");
            wrapper+&renamed.replace("produce(argument())","produce<Int>(argument())")
        };
        c.label=c.label.replacen("temporary-argument","generic-wrapper-argument",1);
        c.source=change(c.source);c.control=change(c.control);c
    }).collect()
}
pub(crate) fn native_sources() -> Vec<(String, String, String)> {
    tuples(cases())
}
fn tuples(cases: Vec<Case>) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for c in cases {
        out.push((format!("{}-no-for", c.label), c.control, c.control_golden));
        let old = "for((key,value) in produce<Int>(argument()))";
        assert_eq!(c.source.matches(old).count(), 1);
        out.push((
            format!("{}-named", c.label),
            c.source.replace(
                old,
                "val source=produce<Int>(argument());for((key,value) in source)",
            ),
            c.golden.clone(),
        ));
        out.push((format!("{}-direct", c.label), c.source, c.golden));
    }
    assert_eq!(out.len(), 6);
    out
}
fn observe(c: &Case, unit: bool, control: bool) -> Result<(), Failure> {
    let text = if control { &c.control } else { &c.source };
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let lowered = if unit {
        pipeline::unit_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            |n, t, o, r| {
                source_forms::facts::unit(&sources, &parsed, n, t, o, r);
                r.query(
                    "generic wrapper original qualified forwarding Call",
                    &wrapper::unit(&sources, &parsed, n, t, o),
                );
                r.query(
                    "generic original ordinary argument and actual overload proof",
                    &facts::unit(c, &sources, &parsed, n, t, o, control),
                );
                let input = ast::input(&sources, &parsed, control).unwrap();
                let unit = t
                    .calls()
                    .iter()
                    .find(|call| call.expression().expression() == input.produce)
                    .unwrap()
                    .expression()
                    .source_unit();
                let call = t
                    .call(lang_frontend::type_checking::UnitExpressionId::new(
                        unit,
                        input.produce,
                    ))
                    .unwrap();
                r.check("generic actual concrete Int type argument",matches!(call.instance().type_arguments(),[ty]if t.types().get(*ty)==Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(lang_frontend::type_checking::BuiltinType::Int))),call);
            },
        )
    } else {
        pipeline::memory_document_observed(&sources, &parsed, &mut report, |n, t, o, r| {
            source_forms::facts::unit(&sources, &parsed, n, t, o, r);
            r.query(
                "generic wrapper original qualified forwarding Call",
                &wrapper::unit(&sources, &parsed, n, t, o),
            );
            r.query(
                "generic original ordinary argument and actual overload proof",
                &facts::unit(c, &sources, &parsed, n, t, o, control),
            );
            let input = ast::input(&sources, &parsed, control).unwrap();
            let unit = t
                .calls()
                .iter()
                .find(|call| call.expression().expression() == input.produce)
                .unwrap()
                .expression()
                .source_unit();
            let call = t
                .call(lang_frontend::type_checking::UnitExpressionId::new(
                    unit,
                    input.produce,
                ))
                .unwrap();
            r.check("generic actual concrete Int type argument",matches!(call.instance().type_arguments(),[ty]if t.types().get(*ty)==Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(lang_frontend::type_checking::BuiltinType::Int))),call);
        })
    };
    diagnostic!(
        "LOCAL GENERIC LOWER label={} unit={unit} control={control} source={text:?} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    report.query(
        "Unit complete temporary source acquisition",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    report.query(
        "generic same existing exact argument/current owner SSA proof",
        &graph::check(c, &sources, &parsed, &program, control),
    );
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    if report.passed() {
        Ok(())
    } else {
        Err(failure("generic exact FE/SSA identities", report))
    }
}
#[test]
fn concrete_return_generic_wrapper_keeps_all_three_shapes() {
    // Preserve the entire original direct-body corpus for the separate 0291
    // hard-success target, including its nine successful v2 observations.
    // This exports identity only; it neither executes nor accepts the three U's.
    let prerequisites =
        crate::ssa::map_provider_exits_pipeline_tests::local_generic_argument_prerequisite_sources(
        );
    assert_eq!(prerequisites.len(), 6);
    let labels = prerequisites
        .iter()
        .map(|(label, _, _)| label)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(labels.len(), 6);
    let originals = [
        (
            "generic-local-argument-Map-borrow-int-overload-no-for",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-no-for.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-no-for.stdout"
            ),
        ),
        (
            "generic-local-argument-Map-borrow-int-overload-named",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-named.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-named.stdout"
            ),
        ),
        (
            "generic-local-argument-Map-borrow-int-overload-direct",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-direct.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-Map-borrow-int-overload-direct.stdout"
            ),
        ),
        (
            "generic-local-argument-MutableMap-borrow-int-overload-no-for",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-no-for.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-no-for.stdout"
            ),
        ),
        (
            "generic-local-argument-MutableMap-borrow-int-overload-named",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-named.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-named.stdout"
            ),
        ),
        (
            "generic-local-argument-MutableMap-borrow-int-overload-direct",
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-direct.ko"
            ),
            include_str!(
                "../../../../tests/fixtures/map-provider-prerequisites/generic-consume/generic-local-argument-MutableMap-borrow-int-overload-direct.stdout"
            ),
        ),
    ];
    for ((label, source, golden), (original_label, original_source, original_golden)) in
        prerequisites.into_iter().zip(originals)
    {
        assert_eq!(label, original_label);
        assert_eq!(
            source, original_source,
            "{label}: preserve original prerequisite source"
        );
        assert_eq!(
            golden, original_golden,
            "{label}: preserve original prerequisite golden"
        );
        assert!(
            source.contains("fun <T> produce(input:Int)")
                && source.contains("produce<Int>(argument())")
        );
        assert!(!golden.is_empty());
        diagnostic!("LOCAL GENERIC PREREQUISITE label={label} source={source:?} golden={golden:?}");
    }
    let mut outcomes = Vec::new();
    for c in cases() {
        for unit in [false, true] {
            for control in [true, false] {
                let r = observe(&c, unit, control);
                diagnostic!(
                    "LOCAL GENERIC CASE label={} unit={unit} control={control} result={r:?}",
                    c.label
                );
                outcomes.push(r);
            }
            let old = "for((key,value) in produce<Int>(argument()))";
            let local = super::Case {
                label: format!("{}-named", c.label),
                source: c.source.replace(
                    old,
                    "val source=produce<Int>(argument());for((key,value) in source)",
                ),
                golden: c.golden.clone(),
                mutable: c.mutable,
                resource: false,
                unit_only: false,
                direct: false,
                route: false,
                delegated: false,
            };
            let r = super::observe(&local, unit);
            diagnostic!(
                "LOCAL GENERIC CASE label={} unit={unit} result={r:?}",
                local.label
            );
            outcomes.push(r);
        }
    }
    assert_eq!(outcomes.len(), 12);
    assert_diagnostic!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
}
