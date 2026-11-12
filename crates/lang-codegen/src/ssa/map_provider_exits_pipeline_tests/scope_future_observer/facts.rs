//! One original analysis, public identities and unmodified cleanup facts.
mod deep {
    //! Exact published cleanup endpoints and current owner identities, from the same analysis.
    use super::*;
    use lang_frontend::{
        name_resolution::ValidatedCompilationUnitNames,
        ownership_checking::{CompilationUnitOwnership, UnitDropPoint, UnitDropTarget},
        parser::Expression,
        type_checking::UnitExpressionId,
    };
    #[derive(Debug)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_future_observer) struct Proof {
        pub final_origin: Option<Span>,
        pub caller_origin: Option<Span>,
    }
    fn final_marker(c: &Case, i: &Input) -> Result<Option<Span>, Failure> {
        match c.code {
            19 => Ok(None),
            20 => Ok(Some(
                i.successor
                    .ok_or_else(|| failure("D actual successor marker", i))?
                    .0,
            )),
            21 => Ok(Some(i.source)),
            _ => unreachable!(),
        }
    }
    pub(super) fn clones(
        p: &ParsedFile,
        i: &Input,
    ) -> Result<Vec<(ExpressionId, ExpressionId)>, Failure> {
        let Some(put) = i.put else {
            return Ok(vec![]);
        };
        let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
        else {
            return Err(failure("D original put Call", put));
        };
        need(
            arguments.len() == 2,
            "D put exact owned key/value slots",
            arguments,
        )?;
        arguments
            .iter()
            .map(|arg| {
                let Expression::Call {
                    callee, arguments, ..
                } = p.ast().expressions().get(arg.value).unwrap().payload()
                else {
                    return Err(failure("D original clone Call", arg));
                };
                let Expression::Member { receiver, .. } =
                    p.ast().expressions().get(*callee).unwrap().payload()
                else {
                    return Err(failure("D actual clone receiver", callee));
                };
                need(
                    arguments.is_empty(),
                    "D clone zero actual arguments",
                    arguments,
                )?;
                Ok((*receiver, arg.value))
            })
            .collect()
    }
    pub(super) fn unit(
        c: &Case,
        p: &ParsedFile,
        i: &Input,
        n: &ValidatedCompilationUnitNames,
        o: &CompilationUnitOwnership,
    ) -> Result<Proof, Failure> {
        let root = super::unit::symbol(n, i.source)?;
        let e = |id| UnitExpressionId::new(root.source_unit(), id);
        for (receiver, clone) in clones(p, i)? {
            let effect = o
                .string_effect(e(clone))
                .ok_or_else(|| failure("D qualified actual clone effect", e(clone)))?;
            let d = one(
                o.drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Temporary(e(receiver))),
                "D qualified independent clone receiver Drop",
            )?;
            need(
                effect.receiver() == e(receiver)
                    && d.point() == UnitDropPoint::CallReturn(e(clone))
                    && d.value_origin() == ast::span(p, receiver),
                "D qualified clone receiver exact effect/CallReturn/origin",
                (effect, d),
            )?;
        }
        let final_origin = final_marker(c, i)?;
        for (marker, call) in final_origin
            .map(|m| (m, i.observes.last().unwrap().0))
            .into_iter()
            .chain(i.result.map(|m| (m, i.entry_observe.unwrap().0)))
        {
            let target = super::unit::symbol(n, marker)?;
            let d = one(
                o.drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Named(target)),
                "D qualified sole final named Drop",
            )?;
            need(
                d.point() == UnitDropPoint::CallReturn(e(call)) && d.value_origin() == marker,
                "D qualified last actual observe exact Drop point and declaration origin",
                d,
            )?;
        }
        Ok(Proof {
            final_origin,
            caller_origin: i.result,
        })
    }
}
mod provider {
    //! Both frontend entrypoints publish the exact original reader and complete normal exits.
    use super::*;
    use lang_frontend::{
        name_resolution::ValidatedCompilationUnitNames,
        ownership_checking::{
            CompilationUnitOwnership, LoanKind, MapIterationComponent, UnitDropPoint,
            UnitIterationCleanupAction as UA, UnitIterationExitKind as UK, UnitLoanTarget,
        },
        type_checking::{CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId},
    };
    pub(super) fn unit(
        p: &ParsedFile,
        i: &Input,
        n: &ValidatedCompilationUnitNames,
        t: &CompilationUnitTypes,
        o: &CompilationUnitOwnership,
    ) -> Result<(), Failure> {
        let s = ast::site(p, i)?;
        let root = super::unit::symbol(n, i.source)?;
        let u = root.source_unit();
        let e = |id| UnitExpressionId::new(u, id);
        let statement = UnitStatementId::new(u, s.statement);
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("D mandatory qualified full provider", statement))?;
        let d = plan.descriptor();
        let target = one(
            n.names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.source_unit() == u && d.name_span() == s.reader_name)
                .map(|d| d.id()),
            "D exact qualified reader declaration",
        )?;
        let call = t
            .call(e(s.read))
            .ok_or_else(|| failure("D qualified actual reader Call", e(s.read)))?;
        need(
            call.target() == UnitCallTarget::Declaration(target)
                && call.receiver().is_none()
                && call.arguments().len() == 2
                && t.types().get(call.return_type())
                    == Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                        BuiltinType::Unit,
                    )),
            "D qualified exact reader target and Unit/two-Borrow ABI",
            call,
        )?;
        for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = super::unit::symbol(n, s.bindings[index])?;
            let published = &plan.loans()[index];
            let loan = one(
                o.loans()
                    .iter()
                    .filter(|l| l.call() == e(s.read) && l.argument() == e(s.arguments[index])),
                "D qualified original reader component loan",
            )?;
            let ty = [d.key_type(), d.value_type()][index];
            need(
                d.bindings()[index] == binding
                    && super::unit::reference(p, n, s.arguments[index])? == binding
                    && o.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && published.statement() == statement
                    && published.source() == e(s.source)
                    && published.binding() == binding
                    && published.component() == component
                    && published.kind() == LoanKind::Shared
                    && Some(published.origin()) == plan.source()
                    && call.arguments()[index].argument_index() == index
                    && call.arguments()[index].parameter_index() == index
                    && call.arguments()[index].mode() == ParameterMode::Borrow
                    && call.arguments()[index].parameter_type() == ty
                    && t.types().get(ty)
                        == Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                            BuiltinType::String,
                        ))
                    && loan.kind() == LoanKind::Shared
                    && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==binding)
                    && loan.begin_span() == ast::span(p, s.arguments[index])
                    && loan.end_span() == ast::span(p, s.read),
                "D qualified canonical source/components/exact slots/loan cleanup",
                (published, loan),
            )?;
        }
        for (kind, point, actions) in [
            (
                UK::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(u, s.body)),
                vec![
                    UA::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UA::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                UK::Exhaustion,
                UnitDropPoint::LoopExit(statement),
                vec![UA::FinishProvider(statement), UA::EndSource(statement)],
            ),
        ] {
            let exit = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "D exact qualified normal provider exit",
            )?;
            need(
                plan.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && o.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "D qualified full reverse components/provider/source exit actions",
                exit,
            )?;
        }
        Ok(())
    }
}
mod unit {
    //! Unit reads only its own source-qualified contracts, delivery and cleanup tables.
    use super::*;
    use crate::test_support::diagnostics::diagnostic;
    use lang_frontend::{
        name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
        ownership_checking::{
            CompilationUnitOwnership, UnitCallArgumentOwnershipKind, UnitDropTarget,
            UnitIterationSourceAccess, UnitLoanTarget, UnitValueDeliveryKind, UnitValueDeliverySource,
        },
        type_checking::{
            CompilationUnitTypes, UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
        },
    };
    pub(super) fn symbol(
        n: &ValidatedCompilationUnitNames,
        marker: Span,
    ) -> Result<UnitSymbolId, Failure> {
        let (u, id) = one(
            n.names().source_units().iter().flat_map(|u| {
                u.resolution()
                    .symbols()
                    .iter()
                    .filter(move |s| s.span() == marker && !s.is_synthetic())
                    .map(move |s| (u.source_unit(), s.id()))
            }),
            "D original qualified symbol marker",
        )?;
        one(
            n.names()
                .references()
                .iter()
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) if s.source_unit() == u && s.symbol() == id => {
                        Some(*s)
                    }
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter(),
            "D original qualified referenced symbol",
        )
    }
    pub(super) fn reference(
        p: &ParsedFile,
        n: &ValidatedCompilationUnitNames,
        e: ExpressionId,
    ) -> Result<UnitSymbolId, Failure> {
        one(
            n.names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(p, e))
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                }),
            "D actual qualified Name reference",
        )
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_future_observer) fn check(
        c: &Case,
        p: &ParsedFile,
        i: &Input,
        n: &ValidatedCompilationUnitNames,
        t: &CompilationUnitTypes,
        o: &CompilationUnitOwnership,
    ) -> Result<Proof, Failure> {
        let u = one(n.names().source_units().iter(), "D original source unit")?.source_unit();
        let e = |id| UnitExpressionId::new(u, id);
        let declaration = |marker| {
            one(
                n.names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| d.source_unit() == u && d.name_span() == marker)
                    .map(|d| d.id()),
                "D original qualified declaration",
            )
        };
        let root = symbol(n, i.source)?;
        let ty = t
            .symbol_type(root)
            .ok_or_else(|| failure("D qualified source type", root))?;
        diagnostic!(
            "SCOPE D FUTURE FE label={} unit=true root={root:?} type={ty:?} formed={:#?} drops={:#?} loans={:#?} contracts={:#?} deliveries={:#?} plans={:#?} puts={:#?} strings={:#?}",
            c.label,
            o.formed_owned_locals(),
            o.drops(),
            o.loans(),
            o.call_argument_contracts(),
            o.value_deliveries(),
            o.map_iterations(),
            t.map_puts(),
            o.string_effects()
        );
        for d in o.drops() {
            diagnostic!(
                "SCOPE D FUTURE DROP label={} unit=true target={:?} point={:?} origin={:?}",
                c.label,
                d.target(),
                d.point(),
                d.value_origin()
            );
        }
        need(
            matches!(t.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor,arguments})if *constructor==if c.code==21{IntrinsicTypeConstructor::MutableMap}else{IntrinsicTypeConstructor::Map}&&arguments.len()==2&&arguments.iter().all(|a|t.types().get(*a)==Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
            "D qualified concrete Map storage",
            ty,
        )?;
        if c.code == 21 {
            need(
                o.formed_owned_locals().contains(&root),
                "D qualified formed MutableMap local",
                root,
            )?;
        } else {
            let scanner = t
                .signatures()
                .declaration(declaration(i.scan_name)?)
                .and_then(|d| d.callable())
                .ok_or_else(|| failure("D original qualified scanner signature", i.scan_name))?;
            diagnostic!(
                "SCOPE D FUTURE PARAMETER label={} unit=true root={root:?} body_mode={:?} binding={:?} signature={scanner:?}",
                c.label,
                t.body_parameter_mode(root),
                o.binding_kind(root)
            );
            need(
                matches!(scanner.parameters(), [parameter] if parameter.symbol() == Some(root)
                    && parameter.mode() == ParameterMode::Value && parameter.ty() == ty)
                    && o.binding_kind(root) == Some(OwnershipBindingKind::Owned),
                "D original qualified own parameter",
                (root, scanner),
            )?;
        }
        for birth in [i.birth, i.entry_birth].into_iter().flatten() {
            let call = t
                .call(e(birth))
                .ok_or_else(|| failure("D qualified factory Call", birth))?;
            need(
                call.target() == UnitCallTarget::Declaration(declaration(i.factory_name)?)
                    && call.arguments().is_empty()
                    && call.receiver().is_none()
                    && call.result_source() == CallableResultSource::Owned
                    && t.expression_type(e(birth)) == Some(ty)
                    && t.expression_category(e(birth)) == Some(ExpressionCategory::Temporary),
                "D qualified selected owned factory result",
                call,
            )?;
        }
        let call = t
            .call(e(i.scan_call))
            .ok_or_else(|| failure("D qualified scanner Call", i.scan_call))?;
        need(
            call.target() == UnitCallTarget::Declaration(declaration(i.scan_name)?)
                && call.receiver().is_none()
                && call.arguments().len() == usize::from(c.code != 21)
                && call.result_source() == CallableResultSource::Owned
                && if c.code == 19 {
                    call.return_type() == ty
                } else {
                    t.types().get(call.return_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                },
            "D qualified actual scanner target",
            call,
        )?;
        if let Some(arg) = i.scan_argument {
            let contract = one(
                o.call_argument_contracts_for(e(i.scan_call))
                    .filter(|c| c.argument() == e(arg)),
                "D actual Value contract",
            )?;
            let delivery = one(
                o.value_deliveries()
                    .iter()
                    .filter(|d| d.call() == e(i.scan_call) && d.argument() == e(arg)),
                "D actual owned temporary delivery",
            )?;
            need(
                matches!(call.arguments(),[a]if a.mode()==ParameterMode::Value&&a.parameter_index()==0&&a.argument_index()==0&&a.parameter_type()==ty)
                    && contract.kind() == UnitCallArgumentOwnershipKind::Value
                    && contract.parameter_type() == ty
                    && delivery.kind() == UnitValueDeliveryKind::Temporary
                    && matches!(delivery.source(),UnitValueDeliverySource::Temporary(value)if *value==e(arg))
                    && !o.loans().iter().any(|l| l.call() == e(i.scan_call)),
                "D qualified Value argument has actual temporary handoff not Borrow",
                (contract, delivery),
            )?;
        }
        if let Some((ret, value)) = i.returned {
            need(
                reference(p, n, value)? == root
                    && t.expression_type(e(value)) == Some(ty)
                    && !o
                        .drops()
                        .iter()
                        .any(|d| d.target() == UnitDropTarget::Named(root)),
                "D qualified actual Return owns source without callee Drop",
                (e(ret), e(value)),
            )?;
        }
        if let Some((marker, init)) = i.successor {
            let next = symbol(n, marker)?;
            need(
                reference(p, n, init)? == root
                    && t.expression_type(e(init)) == Some(ty)
                    && o.formed_owned_locals().contains(&next)
                    && !o
                        .drops()
                        .iter()
                        .any(|d| d.target() == UnitDropTarget::Named(root)),
                "D qualified MoveName successor",
                (next, e(init)),
            )?;
            one(
                o.drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Named(next)),
                "D qualified successor sole Drop",
            )?;
        }
        if c.code == 21 {
            one(
                o.drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Named(root)),
                "D qualified updated MutableMap sole named Drop",
            )?;
        }
        let mut observes = i
            .observes
            .iter()
            .enumerate()
            .map(|(index, &(a, b))| {
                let expected = if c.code == 20 && (c.provider || index == 1) {
                    symbol(
                        n,
                        i.successor
                            .ok_or_else(|| failure("D qualified successor AST", i))?
                            .0,
                    )?
                } else {
                    root
                };
                need(
                    reference(p, n, b)? == expected,
                    "D actual qualified future observe Name identity",
                    b,
                )?;
                Ok((a, b, expected))
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        if let Some((a, b)) = i.entry_observe {
            let result = symbol(
                n,
                i.result
                    .ok_or_else(|| failure("D actual result marker", i))?,
            )?;
            need(
                reference(p, n, b)? == result && o.formed_owned_locals().contains(&result),
                "D qualified actual returned result binding",
                result,
            )?;
            one(
                o.drops()
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Named(result)),
                "D qualified caller result sole Drop",
            )?;
            observes.push((a, b, result));
        }
        for (call, arg, actualroot) in observes {
            let d = t
                .call(e(call))
                .ok_or_else(|| failure("D qualified observe Call", call))?;
            let loan = one(
                o.loans_ending_at(e(call))
                    .filter(|l| l.argument() == e(arg)),
                "D qualified observe loan/end",
            )?;
            let contract = one(
                o.call_argument_contracts_for(e(call))
                    .filter(|c| c.argument() == e(arg)),
                "D qualified observe argument contract",
            )?;
            need(
                d.target() == UnitCallTarget::Declaration(declaration(i.observer_name)?)
                    && matches!(d.arguments(),[a]if a.mode()==ParameterMode::Borrow&&a.parameter_index()==0&&a.argument_index()==0&&a.parameter_type()==ty)
                    && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                    && contract.parameter_type() == ty
                    && matches!(loan.target(),UnitLoanTarget::Place(path)if path.is_root()&&path.root()==actualroot)
                    && loan.begin_span() == ast::span(p, arg)
                    && loan.end_span() == ast::span(p, call),
                "D qualified actual owner Borrow observation and synchronous End",
                (d, loan, contract),
            )?;
        }
        if let Some(put) = i.put {
            let d = t
                .map_put(e(put))
                .ok_or_else(|| failure("D qualified future put", put))?;
            need(
                reference(p, n, d.receiver().expression())? == root
                    && [d.key(), d.value()]
                        .iter()
                        .all(|e| o.string_effect(*e).is_some()),
                "D qualified actual put root and clone ownership effects",
                d,
            )?;
        }
        need(
            t.map_iterations().len() == usize::from(c.provider)
                && o.map_iterations().len() == usize::from(c.provider),
            "D qualified exact acquired plans",
            o.map_iterations(),
        )?;
        if let Some((statement, source)) = i.site {
            let statement = UnitStatementId::new(u, statement);
            let plan = o
                .map_iteration(statement)
                .ok_or_else(|| failure("D qualified provider plan", statement))?;
            need(
                t.map_iteration(statement) == Some(plan.descriptor())
                    && plan.descriptor().source() == e(source)
                    && plan.descriptor().source_type() == ty
                    && reference(p, n, source)? == root
                    && matches!(plan.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==root)
                    && plan.source_access() == UnitIterationSourceAccess::Owned
                    && plan.loans().len() == 2
                    && !plan.exits().is_empty(),
                "D qualified owned original provider",
                plan,
            )?;
        }
        if c.provider {
            super::provider::unit(p, i, n, t, o)?;
        }
        super::deep::unit(c, p, i, n, o)
    }
}
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{self, Input},
};
pub(super) use deep::Proof;
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::OwnershipBindingKind,
    parser::ParsedFile,
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, ExpressionCategory, IntrinsicTypeConstructor,
        ParameterMode,
    },
};
pub(super) use unit::check as unit;

pub(crate) mod packed_provider_cfg_3 {
//! Shared provider CFG and native cleanup across element layouts.
use crate::llvm::container_tests::*;

#[test]
fn borrowed_provider_cfg_lowers_length_once_and_cleanup_exits() {
    #[derive(Clone, Copy)]
    enum ElementCase {
        Int,
        Empty,
        MoveOnlyToken,
    }
    for (label, case) in [
        ("provider-int", ElementCase::Int),
        ("provider-empty", ElementCase::Empty),
        ("provider-token", ElementCase::MoveOnlyToken),
    ] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module(label);
        let module = program.module_mut(module_id).expect("module exists");
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let element = match case {
            ElementCase::Int => integer,
            ElementCase::Empty => module
                .add_aggregate_type("Empty", Vec::new())
                .expect("empty aggregate"),
            ElementCase::MoveOnlyToken => module.intern_type(SsaTypeKind::ZeroSized {
                name: "Token".to_owned(),
                ownership: Ownership::MoveOnly,
            }),
        };
        let array = module
            .add_sequential_container_type(SequentialContainerKind::Array, element)
            .expect("Array<Element>");
        let (iterate, entry, parameters) =
            add_function(module, "iterate", &[array, boolean], vec![array], &origin);
        let function = module.function_mut(iterate).expect("iterate exists");
        let source_type = EntityType::Loan {
            kind: LoanKind::Shared,
            target: array,
        };
        let state_types = vec![
            EntityType::Value(array),
            source_type,
            EntityType::Value(integer),
            EntityType::Value(integer),
            EntityType::Value(boolean),
        ];
        let header = function
            .add_block(state_types.clone(), origin.clone())
            .expect("header");
        let body = function
            .add_block(state_types, origin.clone())
            .expect("body");
        let exit = function
            .add_block(vec![EntityType::Value(array), source_type], origin.clone())
            .expect("exit");
        let early_return = if matches!(case, ElementCase::MoveOnlyToken) {
            Some(
                function
                    .add_block(vec![EntityType::Value(array), source_type], origin.clone())
                    .expect("early return"),
            )
        } else {
            None
        };
        let source_place = append_place(
            function,
            entry,
            Operation::RootPlace {
                owner: parameters[0],
            },
            array,
            &origin,
        );
        let source_loan = loan(
            function
                .append_instruction(
                    entry,
                    Operation::BorrowBegin {
                        place: source_place,
                        kind: LoanKind::Shared,
                    },
                    vec![source_type],
                    origin.clone(),
                )
                .expect("source loan")
                .1[0],
        );
        let snapshot = provider::snapshot(
            function,
            entry,
            source_loan,
            integer,
            &origin,
            lang_frontend::type_checking::IterationProvider::Sequential(
                lang_frontend::type_checking::SequentialContainerKind::Array,
            ),
        )
        .expect("provider snapshot");
        let (snapshot_length, snapshot_cursor) = (snapshot.length(), snapshot.cursor());
        let provider_header = provider::enter_header(
            function,
            entry,
            header,
            snapshot,
            vec![
                EntityId::Value(parameters[0]),
                EntityId::Loan(source_loan),
                EntityId::Value(snapshot_cursor),
                EntityId::Value(snapshot_length),
                EntityId::Value(parameters[1]),
            ],
            1,
            2,
            3,
            &origin,
        )
        .expect("provider entry fixes length/cursor slots");

        let header_params = &function.block(header).expect("header").parameters;
        let (header_owner, header_loan, header_length, header_cursor, header_stop) = (
            value(header_params[0]),
            loan(header_params[1]),
            value(header_params[2]),
            value(header_params[3]),
            value(header_params[4]),
        );
        let element = provider::guard_and_begin(
            function,
            &provider_header,
            boolean,
            element,
            Edge {
                target: body,
                arguments: vec![
                    EntityId::Value(header_owner),
                    EntityId::Loan(header_loan),
                    EntityId::Value(header_length),
                    EntityId::Value(header_cursor),
                    EntityId::Value(header_stop),
                ],
            },
            Edge {
                target: exit,
                arguments: vec![EntityId::Value(header_owner), EntityId::Loan(header_loan)],
            },
            &origin,
            lang_frontend::type_checking::IterationProvider::Sequential(
                lang_frontend::type_checking::SequentialContainerKind::Array,
            ),
        )
        .expect("guarded provider element");

        let body_params = &function.block(body).expect("body").parameters;
        let (body_owner, body_loan, body_length, body_cursor, body_stop) = (
            value(body_params[0]),
            loan(body_params[1]),
            value(body_params[2]),
            value(body_params[3]),
            value(body_params[4]),
        );
        let next = provider::finish_element_and_advance(function, element, integer, &origin)
            .expect("next cursor");
        let next_value = next.value();
        function
            .set_terminator(
                body,
                TerminatorKind::Conditional {
                    condition: body_stop,
                    when_true: Edge {
                        target: early_return.unwrap_or(exit),
                        arguments: vec![EntityId::Value(body_owner), EntityId::Loan(body_loan)],
                    },
                    when_false: provider_header
                        .backedge(
                            vec![
                                EntityId::Value(body_owner),
                                EntityId::Loan(body_loan),
                                EntityId::Value(next_value),
                                EntityId::Value(body_length),
                                EntityId::Value(body_stop),
                            ],
                            next,
                        )
                        .expect("provider backedge fixes length/cursor slots"),
                },
                origin.clone(),
            )
            .expect("body branch");

        let exit_params = &function.block(exit).expect("exit").parameters;
        let exit_owner = value(exit_params[0]);
        let exit_loan = loan(exit_params[1]);
        function
            .append_instruction(
                exit,
                Operation::BorrowEnd { loan: exit_loan },
                Vec::new(),
                origin.clone(),
            )
            .expect("source loan end");
        ret(function, exit, vec![exit_owner], &origin);
        if let Some(early_return) = early_return {
            let params = &function
                .block(early_return)
                .expect("early return")
                .parameters;
            let (owner, source_loan) = (value(params[0]), loan(params[1]));
            function
                .append_instruction(
                    early_return,
                    Operation::BorrowEnd { loan: source_loan },
                    Vec::new(),
                    origin.clone(),
                )
                .expect("early return source loan end");
            ret(function, early_return, vec![owner], &origin);
        }

        let entry_ops = function
            .block(entry)
            .expect("entry")
            .instructions
            .iter()
            .map(|id| &function.instruction(*id).expect("instruction").operation)
            .collect::<Vec<_>>();
        assert_eq!(
            entry_ops
                .iter()
                .filter(|op| matches!(op, Operation::ContainerLength { .. }))
                .count(),
            1
        );
        assert!(entry_ops.iter().any(|op| matches!(
            op,
            Operation::Constant(crate::ssa::model::ScalarConstant::Integer(0))
        )));
        assert!(entry_ops.iter().any(|op| matches!(
            op,
            Operation::Constant(crate::ssa::model::ScalarConstant::Integer(1))
        )));
        let TerminatorKind::Branch(entry_edge) = &function
            .block(entry)
            .expect("entry")
            .terminator
            .as_ref()
            .expect("entry branch")
            .kind
        else {
            panic!("provider must enter header once");
        };
        assert_eq!(entry_edge.target, header);
        assert_eq!(entry_edge.arguments[2], EntityId::Value(snapshot_length));
        assert_eq!(entry_edge.arguments[3], EntityId::Value(snapshot_cursor));
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &function
            .block(header)
            .expect("header")
            .terminator
            .as_ref()
            .expect("guard")
            .kind
        else {
            panic!("provider header must guard its body");
        };
        assert_eq!(when_true.target, body);
        assert_eq!(when_false.target, exit);
        let header_ops = function
            .block(header)
            .expect("header")
            .instructions
            .iter()
            .map(|id| &function.instruction(*id).expect("instruction").operation)
            .collect::<Vec<_>>();
        assert!(matches!(
            header_ops.as_slice(),
            [Operation::Compare {
                operator: crate::ssa::model::ComparisonOperator::LessThan,
                left,
                right,
            }] if *left == header_cursor && *right == header_length
        ));
        let body_ops = function
            .block(body)
            .expect("body")
            .instructions
            .iter()
            .map(|id| &function.instruction(*id).expect("instruction").operation)
            .collect::<Vec<_>>();
        assert!(matches!(
            body_ops[0],
            Operation::ContainerElementPlace { owner, index }
                if *owner == EntityId::Loan(body_loan) && *index == body_cursor
        ));
        assert!(matches!(body_ops[1], Operation::BorrowBegin { .. }));
        assert!(matches!(body_ops[2], Operation::BorrowEnd { .. }));
        assert!(matches!(
            body_ops[3],
            Operation::Binary {
                operator: crate::ssa::model::BinaryOperator::Add,
                left,
                right,
            } if *left == body_cursor && *right == provider_header.step()
        ));
        let TerminatorKind::Conditional { when_false, .. } = &function
            .block(body)
            .expect("body")
            .terminator
            .as_ref()
            .expect("body branch")
            .kind
        else {
            panic!("provider body must branch");
        };
        assert_eq!(when_false.target, header);
        assert_eq!(when_false.arguments[2], EntityId::Value(header_length));
        assert_eq!(when_false.arguments[3], EntityId::Value(next_value));

        verify_program(&program).expect("provider CFG must satisfy SSA ownership");
        let first = crate::test_support::without_abort_runtime(
            &render_verified_program(&program).expect("provider LLVM must verify"),
        );
        let second = crate::test_support::without_abort_runtime(
            &render_verified_program(&program).expect("provider LLVM must be deterministic"),
        );
        crate::test_support::diagnostics::assert_text_eq!(first, second);
        let entry_ir = first
            .split("bb0:")
            .nth(1)
            .expect("entry block")
            .split("bb1:")
            .next()
            .expect("entry end");
        assert_eq!(entry_ir.matches("extractvalue %koven.container").count(), 1);
        let exit_header = first
            .lines()
            .find(|line| line.starts_with("bb3:"))
            .expect("exit block");
        assert!(exit_header.contains("%bb1"));
        match case {
            ElementCase::MoveOnlyToken => {
                assert!(!exit_header.contains("%p1.valid"));
                assert_eq!(first.matches("ret %koven.container").count(), 2);
            }
            ElementCase::Int | ElementCase::Empty => {
                assert!(exit_header.contains("%p1.valid"));
                assert_eq!(first.matches("ret %koven.container").count(), 1);
            }
        }
        assert!(first.contains("icmp slt i32"), "{first}");
        assert!(first.contains("icmp sge i32"), "{first}");
        match case {
            ElementCase::Int => assert!(first.contains("getelementptr i32, ptr"), "{first}"),
            ElementCase::Empty | ElementCase::MoveOnlyToken => {
                assert!(!first.contains("getelementptr"), "{first}");
            }
        }
        assert!(!first.contains("call ptr @malloc"), "{first}");
        assert!(!first.contains("@koven.iterator"), "{first}");
    }
}
}
