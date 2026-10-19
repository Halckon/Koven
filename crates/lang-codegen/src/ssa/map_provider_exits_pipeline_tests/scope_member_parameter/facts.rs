//! Source-qualified member selection, receiver delivery and the original borrowed source lease.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{self, Input},
};
use crate::ssa::unit_plan;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropFact, UnitDropPoint,
        UnitDropTarget, UnitIterationCleanupAction as A, UnitIterationExitKind as E,
        UnitIterationSourceAccess, UnitLoanTarget, UnitReceiverOwnershipKind,
        UnitReceiverOwnershipTarget,
    },
    parser::ParsedFile,
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, Copyability, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, UnitCallReceiverOrigin, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeId, UnitTypeKind,
    },
};
#[derive(Debug)]
pub(super) struct Proof {
    pub map_drop: UnitDropFact,
    pub receiver_drop: UnitDropFact,
    pub receiver_begin: Span,
    pub receiver_end: Span,
}
struct Facts<'a> {
    p: &'a ParsedFile,
    n: &'a ValidatedCompilationUnitNames,
    t: &'a CompilationUnitTypes,
    o: &'a CompilationUnitOwnership,
    unit: lang_frontend::name_resolution::SourceUnitId,
}
impl Facts<'_> {
    fn e(&self, id: ExpressionId) -> UnitExpressionId {
        UnitExpressionId::new(self.unit, id)
    }
    fn reference(&self, expression: ExpressionId) -> Result<UnitSymbolId, Failure> {
        let r = one(
            self.n.names().references().iter().filter(|r| {
                r.source_unit() == self.unit && r.span() == ast::span(self.p, expression)
            }),
            "P24 exact original qualified reference",
        )?;
        match r.target() {
            UnitReferenceTarget::Symbol(s) => Ok(*s),
            _ => Err(failure("P24 original reference has a real symbol", r)),
        }
    }
    fn symbol_span(&self, symbol: UnitSymbolId) -> Result<Span, Failure> {
        need(
            symbol.source_unit() == self.unit,
            "P24 symbol belongs to original source unit",
            symbol,
        )?;
        self.n
            .names()
            .source_units()
            .iter()
            .find(|u| u.source_unit() == self.unit)
            .and_then(|u| {
                u.resolution()
                    .symbols()
                    .iter()
                    .find(|s| s.id() == symbol.symbol() && !s.is_synthetic())
            })
            .map(|s| s.span())
            .ok_or_else(|| failure("P24 original symbol marker", symbol))
    }
    fn declaration(&self, span: Span) -> Result<DeclarationId, Failure> {
        one(
            self.n
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| {
                    d.source_unit() == self.unit
                        && self
                            .p
                            .ast()
                            .items()
                            .get(d.root())
                            .is_ok_and(|n| n.span() == span)
                })
                .map(|d| d.id()),
            "P24 exact original AST root declaration",
        )
    }
    fn drop(&self, symbol: UnitSymbolId) -> Result<UnitDropFact, Failure> {
        one(
            self.o
                .drops()
                .iter()
                .copied()
                .filter(|d| d.target() == UnitDropTarget::Named(symbol)),
            "P24 unique original caller named Drop",
        )
    }
    fn borrowed_call(
        &self,
        call: ExpressionId,
        target: UnitCallTarget,
        arguments: &[(ExpressionId, UnitSymbolId, UnitTypeId)],
    ) -> Result<(), Failure> {
        let d = self
            .t
            .call(self.e(call))
            .ok_or_else(|| failure("P24 actual Borrow call descriptor", call))?;
        let contracts = self
            .o
            .call_argument_contracts_for(self.e(call))
            .collect::<Vec<_>>();
        let loans = self.o.loans_ending_at(self.e(call)).collect::<Vec<_>>();
        need(
            d.expression() == self.e(call)
                && d.target() == target
                && d.arguments().len() == arguments.len()
                && contracts.len() == arguments.len()
                && loans.len() == arguments.len(),
            "P24 complete actual call arguments/contracts/loan ends",
            (d, &contracts, &loans),
        )?;
        for (index, &(argument, root, ty)) in arguments.iter().enumerate() {
            let mapping = d.arguments()[index];
            let contract = one(
                contracts
                    .iter()
                    .copied()
                    .filter(|c| c.argument() == self.e(argument)),
                "P24 original argument ownership contract",
            )?;
            let loan = one(
                loans
                    .iter()
                    .copied()
                    .filter(|l| l.argument() == self.e(argument)),
                "P24 original argument loan end",
            )?;
            need(
                self.reference(argument)? == root
                    && self.t.symbol_type(root) == Some(ty)
                    && self.t.expression_type(self.e(argument)) == Some(ty)
                    && self.t.expression_category(self.e(argument))
                        == Some(ExpressionCategory::Place)
                    && mapping.argument_index() == index
                    && mapping.parameter_index() == index
                    && mapping.mode() == ParameterMode::Borrow
                    && mapping.parameter_type() == ty
                    && !mapping.crosses_thread()
                    && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                    && contract.parameter_index() == index
                    && contract.parameter_type() == ty
                    && !contract.crosses_thread()
                    && loan.call() == self.e(call)
                    && loan.kind() == LoanKind::Shared
                    && loan.end_span() == ast::span(self.p, call)
                    && matches!(loan.target(),UnitLoanTarget::Place(p)if p.is_root()&&p.root()==root),
                "P24 exact ordered Borrow argument keeps its original root and CallReturn",
                (mapping, contract, loan),
            )?;
        }
        Ok(())
    }
}
pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    t: &CompilationUnitTypes,
    o: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let unit = one(n.names().source_units().iter(), "P24 original source unit")?.source_unit();
    let f = Facts { p, n, t, o, unit };
    let class = f.declaration(p.ast().items().get(i.class).unwrap().span())?;
    let nominal = t
        .signatures()
        .declaration(class)
        .and_then(|d| d.nominal())
        .ok_or_else(|| failure("P24 actual Reader nominal signature", class))?;
    need(
        f.symbol_span(nominal.symbol())? == i.class_name,
        "P24 Reader signature refers to original classifier",
        nominal,
    )?;
    let call = t
        .call(f.e(i.member_call))
        .ok_or_else(|| failure("P24 original reader.scan selected call", i.member_call))?;
    let UnitCallTarget::Symbol(method) = call.target() else {
        return Err(failure(
            "P24 member has source-qualified Symbol target",
            call,
        ));
    };
    let sig = one(
        nominal
            .members()
            .iter()
            .filter(|s| s.target() == UnitCallableTarget::Symbol(method)),
        "P24 signature selected from actual Reader members",
    )?;
    let [parameter] = sig.parameters() else {
        return Err(failure("P24 method one explicit qualified parameter", sig));
    };
    let root = parameter
        .symbol()
        .ok_or_else(|| failure("P24 actual source parameter symbol", parameter))?;
    let receiver = sig
        .receiver()
        .ok_or_else(|| failure("P24 real hidden receiver contract", sig))?;
    need(
        f.symbol_span(method)? == i.method_name
            && sig.name_span() == i.method_name
            && p.ast().items().get(i.method).unwrap().span() == i.method_span
            && f.symbol_span(root)? == i.parameter
            && parameter.mode() == ParameterMode::Borrow
            && receiver.mode() == ParameterMode::Borrow
            && receiver.declaration_span() == i.method_name
            && sig.type_parameters().is_empty()
            && sig.has_body()
            && t.types().get(sig.return_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            && o.binding_kind(root) == Some(OwnershipBindingKind::Shared),
        "P24 actual method/receiver/source Borrow parameter identity",
        sig,
    )?;
    let ty = parameter.ty();
    let Some(UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::Map,
        arguments,
    }) = t.types().get(ty)
    else {
        return Err(failure("P24 concrete Map parameter", ty));
    };
    need(
        arguments.len() == 2
            && arguments
                .iter()
                .all(|v| t.types().get(*v) == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
            && t.copyability(ty) == Copyability::MoveOnly,
        "P24 represented String/String Map",
        ty,
    )?;
    let receiver_root = f.reference(i.receiver)?;
    let caller_root = f.reference(i.argument)?;
    need(
        f.symbol_span(receiver_root)? == i.receiver_name
            && f.symbol_span(caller_root)? == i.source_name
            && t.expression_type(f.e(i.receiver_birth)) == Some(receiver.ty())
            && t.construction(f.e(i.receiver_birth)).is_some()
            && matches!(t.types().get(receiver.ty()),Some(UnitTypeKind::Nominal{declaration,arguments})if *declaration==class&&arguments.is_empty())
            && t.symbol_type(receiver_root) == Some(receiver.ty())
            && t.symbol_type(caller_root) == Some(ty)
            && o.formed_owned_locals().contains(&caller_root),
        "P24 caller receiver construction and independently owned Map root",
        (receiver_root, caller_root),
    )?;
    let actual_receiver = call
        .receiver()
        .ok_or_else(|| failure("P24 actual member receiver", call))?;
    let receiver_fact = o
        .receiver_fact(f.e(i.member_call))
        .ok_or_else(|| failure("P24 public receiver effect", call))?;
    let receiver_contract = o
        .call_receiver_contract(f.e(i.member_call))
        .ok_or_else(|| failure("P24 public receiver contract", call))?;
    need(
        actual_receiver.origin() == UnitCallReceiverOrigin::Expression(f.e(i.receiver))
            && actual_receiver.mode() == ParameterMode::Borrow
            && actual_receiver.ty() == receiver.ty()
            && actual_receiver.category() == ExpressionCategory::Place
            && receiver_fact.call() == f.e(i.member_call)
            && receiver_fact.source() == actual_receiver.origin()
            && receiver_fact.kind() == UnitReceiverOwnershipKind::SharedLoan
            && !receiver_fact.is_receiver_reservation()
            && matches!(receiver_fact.target(),UnitReceiverOwnershipTarget::Place(place)if place.is_root()&&place.root()==receiver_root)
            && receiver_contract.source() == actual_receiver.origin()
            && receiver_contract.target() == call.target()
            && receiver_contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
            && receiver_contract.receiver_type() == receiver.ty()
            && receiver_contract.receiver_span() == ast::span(p, i.receiver)
            && receiver_contract.call_span() == ast::span(p, i.member_call),
        "P24 original receiver source and synchronous Shared delivery",
        (receiver_fact, receiver_contract),
    )?;
    f.borrowed_call(
        i.member_call,
        UnitCallTarget::Symbol(method),
        &[(i.argument, caller_root, ty)],
    )?;
    let resolved = unit_plan::resolve_unit_call_instance(
        t,
        o,
        sig.target(),
        call.instance().type_arguments().to_vec(),
        Some(receiver.ty()),
        ast::span(p, i.member_call),
    )
    .map_err(|e| failure("P24 actual selected member instance", e))?;
    need(
        resolved.key().target() == sig.target()
            && resolved.delegation().is_empty()
            && call.return_type() == sig.return_type()
            && call.result_source() == CallableResultSource::Owned
            && i.member_name != i.method_name,
        "P24 actual unredirected member instance and distinct use marker",
        resolved.key(),
    )?;
    let factory = t
        .call(f.e(i.source_birth))
        .ok_or_else(|| failure("P24 original make result", i.source_birth))?;
    need(
        factory.target() == UnitCallTarget::Declaration(f.declaration(i.make)?)
            && factory.receiver().is_none()
            && factory.arguments().is_empty()
            && factory.return_type() == ty
            && factory.result_source() == CallableResultSource::Owned
            && t.expression_category(f.e(i.source_birth)) == Some(ExpressionCategory::Temporary),
        "P24 actual caller Map birth",
        factory,
    )?;
    need(
        !o.drops().iter().any(|d| {
            d.target() == UnitDropTarget::Named(root) || d.target() == UnitDropTarget::This(class)
        }) && t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "P24 Borrow method never owns/drops caller source or receiver",
        o.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let statement = UnitStatementId::new(unit, site.statement);
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("P24 real method source plan", statement))?;
        let d = plan.descriptor();
        need(
            t.map_iteration(statement) == Some(d)
                && d.source() == f.e(site.source)
                && f.reference(site.source)? == root
                && d.source_type() == ty
                && plan.source_access() == UnitIterationSourceAccess::Shared
                && matches!(plan.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(place))if place.is_root()&&place.root()==root),
            "P24 exact source-qualified borrowed provider source",
            plan,
        )?;
        let mut body_arguments = Vec::new();
        for index in 0..2 {
            let binding = d.bindings()[index];
            let loan = &plan.loans()[index];
            need(
                f.symbol_span(binding)? == site.bindings[index]
                    && plan.bindings()[index].symbol() == binding
                    && plan.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && o.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && loan.binding() == binding
                    && loan.statement() == statement
                    && loan.source() == d.source()
                    && Some(loan.origin()) == plan.source()
                    && loan.component()
                        == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                    && loan.kind() == LoanKind::Shared,
                "P24 actual independent ordered component facts",
                loan,
            )?;
            body_arguments.push((
                site.arguments[index],
                binding,
                [d.key_type(), d.value_type()][index],
            ));
        }
        f.borrowed_call(
            site.read,
            UnitCallTarget::Declaration(f.declaration(i.reader)?),
            &body_arguments,
        )?;
        need(
            plan.exits().len() == 2,
            "P24 complete normal provider exits",
            plan.exits(),
        )?;
        for (kind, point, actions) in [
            (
                E::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
                vec![
                    A::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    A::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                E::Exhaustion,
                UnitDropPoint::LoopExit(statement),
                vec![A::FinishProvider(statement), A::EndSource(statement)],
            ),
        ] {
            let exit = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "P24 original exact exit",
            )?;
            need(
                exit.point() == point
                    && exit.actions() == actions
                    && o.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "P24 complete component/provider/source cleanup order",
                exit,
            )?;
        }
    } else {
        let (call, arg) = i
            .control
            .ok_or_else(|| failure("P24 original control Call", i))?;
        f.borrowed_call(
            call,
            UnitCallTarget::Declaration(f.declaration(i.observe)?),
            &[(arg, root, ty)],
        )?;
    }
    let map_drop = f.drop(caller_root)?;
    let receiver_drop = f.drop(receiver_root)?;
    need(
        [map_drop, receiver_drop]
            .iter()
            .all(|d| d.point() == UnitDropPoint::CallReturn(f.e(i.member_call)))
            && map_drop.value_origin() == i.source_name
            && receiver_drop.value_origin() == i.receiver_name,
        "P24 caller retains both owners until actual member CallReturn",
        (map_drop, receiver_drop),
    )?;
    Ok(Proof {
        map_drop,
        receiver_drop,
        receiver_begin: receiver_fact.begin_span(),
        receiver_end: receiver_fact.end_span(),
    })
}
