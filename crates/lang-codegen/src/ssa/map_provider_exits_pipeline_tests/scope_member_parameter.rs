//! P24 keeps the original Unit member receiver and independent Borrow Map parameter.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast {
    //! The method comes from its original classifier body, never a top-level name match.
    use super::super::{
        births::ast::Site,
        fixture::{Failure, failure, need},
        query_inputs::one,
    };
    use super::Case;
    use lang_frontend::{
        ast::{ExpressionId, ItemId},
        parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
        source::{SourceMap, Span},
    };
    #[derive(Debug)]
    pub(super) struct Input {
        pub class: ItemId,
        pub class_name: Span,
        pub method: ItemId,
        pub method_span: Span,
        pub method_name: Span,
        pub parameter: Span,
        pub entry: Span,
        pub make: Span,
        pub reader: Span,
        pub observe: Span,
        pub receiver_name: Span,
        pub receiver_birth: ExpressionId,
        pub source_name: Span,
        pub source_birth: ExpressionId,
        pub member_call: ExpressionId,
        pub receiver: ExpressionId,
        pub argument: ExpressionId,
        pub member_name: Span,
        pub site: Option<Site>,
        pub control: Option<(ExpressionId, ExpressionId)>,
    }
    pub(super) fn span(p: &ParsedFile, e: ExpressionId) -> Span {
        p.ast().expressions().get(e).unwrap().span()
    }
    fn contains(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() < inner.start()
            && inner.end() < outer.end()
    }
    fn top(s: &SourceMap, p: &ParsedFile, name: &str, arity: usize) -> Result<Span, Failure> {
        one(p.roots().iter().filter_map(|id| {
            let n = p.ast().items().get(*id).ok()?;
            matches!(n.payload(), Item::Function { name: NameMarker::Present(marker), parameters, .. }
                if s.slice(*marker) == Ok(name) && parameters.len() == arity).then_some(n.span())
        }), "P24 original top-level callable AST/arity")
    }
    fn call(
        s: &SourceMap,
        p: &ParsedFile,
        scope: Span,
        name: &str,
    ) -> Result<ExpressionId, Failure> {
        one(
            p.ast().expressions().iter().filter_map(|(id, n)| {
                let Expression::Call { callee, .. } = n.payload() else {
                    return None;
                };
                (contains(scope, n.span()) && s.slice(span(p, *callee)) == Ok(name)).then_some(id)
            }),
            "P24 original Call inside actual callable",
        )
    }
    fn local(
        s: &SourceMap,
        p: &ParsedFile,
        scope: Span,
        name: &str,
    ) -> Result<(Span, ExpressionId), Failure> {
        one(
            p.ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(marker),
                        initializer,
                        ..
                    } if contains(scope, n.span()) && s.slice(*marker) == Ok(name) => {
                        Some((*marker, *initializer))
                    }
                    _ => None,
                }),
            "P24 original caller local declaration/initializer",
        )
    }
    pub(super) fn input(c: &Case, s: &SourceMap, p: &ParsedFile) -> Result<Input, Failure> {
        let (class, class_name, method) = one(
            p.roots().iter().filter_map(|id| {
                let Item::Classifier(class) = p.ast().items().get(*id).ok()?.payload() else {
                    return None;
                };
                let NameMarker::Present(name) = class.name else {
                    return None;
                };
                if s.slice(name) != Ok("Reader") {
                    return None;
                }
                let [method] = class.body.as_ref()?.members.as_slice() else {
                    return None;
                };
                Some((*id, name, *method))
            }),
            "P24 actual Reader classifier and sole real member",
        )?;
        let method_node = p.ast().items().get(method).unwrap();
        let Item::Function {
            name: NameMarker::Present(method_name),
            parameters,
            extension_receiver: None,
            ..
        } = method_node.payload()
        else {
            return Err(failure("P24 original instance function AST", method_node));
        };
        let [parameter] = parameters.as_slice() else {
            return Err(failure(
                "P24 method exactly one explicit parameter",
                parameters,
            ));
        };
        let NameMarker::Present(parameter_name) = parameter.name else {
            return Err(failure("P24 real Map parameter marker", parameter));
        };
        need(
            s.slice(*method_name) == Ok("scan")
                && parameter.mode_marker.is_none()
                && s.slice(p.ast().type_refs().get(parameter.type_ref).unwrap().span())
                    == Ok("Map<String,String>"),
            "P24 real method/default Borrow Map syntax",
            method_node,
        )?;
        let entry = top(s, p, "entry", 0)?;
        let (receiver_name, receiver_birth) = local(s, p, entry, "reader")?;
        let (source_name, source_birth) = local(s, p, entry, "source")?;
        let member_call = call(s, p, entry, "reader.scan")?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = p.ast().expressions().get(member_call).unwrap().payload()
        else {
            unreachable!()
        };
        let Expression::Member {
            receiver,
            name_span,
            safe: false,
            ..
        } = p.ast().expressions().get(*callee).unwrap().payload()
        else {
            return Err(failure("P24 actual nonnullable member callee", callee));
        };
        let [argument] = arguments.as_slice() else {
            return Err(failure("P24 actual member one argument", arguments));
        };
        need(
            type_arguments.is_empty()
                && argument.named_prefix.is_none()
                && argument.mode_marker.is_none(),
            "P24 original plain member argument",
            arguments,
        )?;
        let site = if c.provider {
            let (statement, n, names, source, body) = one(
                p.ast()
                    .statements()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Statement::For {
                            binding: ForBinding::Destructuring { names, .. },
                            source,
                            body,
                            ..
                        } if contains(method_node.span(), n.span()) => {
                            Some((id, n, names, *source, *body))
                        }
                        _ => None,
                    }),
                "P24 For is inside this actual method",
            )?;
            let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
                return Err(failure("P24 original K/V markers", names));
            };
            let read = call(s, p, n.span(), "read")?;
            let Expression::Call { arguments, .. } =
                p.ast().expressions().get(read).unwrap().payload()
            else {
                unreachable!()
            };
            let [key_arg, value_arg] = arguments.as_slice() else {
                return Err(failure("P24 actual read arity", arguments));
            };
            Some(Site {
                statement,
                span: n.span(),
                source,
                body,
                bindings: [*key, *value],
                read,
                arguments: [key_arg.value, value_arg.value],
            })
        } else {
            None
        };
        let control = if c.provider {
            None
        } else {
            let observed = call(s, p, method_node.span(), "observe")?;
            let Expression::Call { arguments, .. } =
                p.ast().expressions().get(observed).unwrap().payload()
            else {
                unreachable!()
            };
            let [argument] = arguments.as_slice() else {
                return Err(failure("P24 original control observe argument", arguments));
            };
            Some((observed, argument.value))
        };
        Ok(Input {
            class,
            class_name,
            method,
            method_span: method_node.span(),
            method_name: *method_name,
            parameter: parameter_name,
            entry,
            make: top(s, p, "make", 0)?,
            reader: top(s, p, "read", 2)?,
            observe: top(s, p, "observe", 1)?,
            receiver_name,
            receiver_birth,
            source_name,
            source_birth,
            member_call,
            receiver: *receiver,
            argument: argument.value,
            member_name: *name_span,
            site,
            control,
        })
    }
}
mod facts {
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
            BuiltinType, CallableResultSource, CompilationUnitTypes, Copyability,
            ExpressionCategory, IntrinsicTypeConstructor, ParameterMode, UnitCallReceiverOrigin,
            UnitCallTarget, UnitCallableTarget, UnitExpressionId, UnitStatementId, UnitTypeId,
            UnitTypeKind,
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
                && t.types().get(sig.return_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
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
                && arguments.iter().all(|v| {
                    t.types().get(*v) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                })
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
                && t.expression_category(f.e(i.source_birth))
                    == Some(ExpressionCategory::Temporary),
            "P24 actual caller Map birth",
            factory,
        )?;
        need(
            !o.drops().iter().any(|d| {
                d.target() == UnitDropTarget::Named(root)
                    || d.target() == UnitDropTarget::This(class)
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
}
mod graph;
use super::{
    Report,
    fixture::{Failure, clean, failure, need},
    pipeline,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: &'static str,
    pub source: &'static str,
    pub golden: &'static str,
    provider: bool,
}
pub(crate) fn sources() -> [Case; 2] {
    [
        Case {
            label: "P24-for",
            provider: true,
            source: include_str!("../../../tests/fixtures/map-provider-scope-0331/P24-for.ko"),
            golden: include_str!("../../../tests/fixtures/map-provider-scope-0331/P24-for.stdout"),
        },
        Case {
            label: "P24-control",
            provider: false,
            source: include_str!("../../../tests/fixtures/map-provider-scope-0331/P24-control.ko"),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-scope-0331/P24-control.stdout"
            ),
        },
    ]
}
fn observe(c: &Case) -> Result<(), Failure> {
    diagnostic!(
        "SCOPE MEMBER INPUT label={} unit=true source={:?} golden={:?}",
        c.label,
        c.source,
        c.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", c.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let input = ast::input(c, &sources, &parsed)?;
    let mut report = Report::default();
    let mut proof = None;
    let mut observed = false;
    let lowered = pipeline::unit_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        |n, t, o, r| {
            observed = true;
            let result = facts::check(c, &parsed, &input, n, t, o);
            r.query(
                "P24 actual member receiver/parameter/call/source plan/complete cleanup",
                &result,
            );
            proof = result.ok();
        },
    );
    diagnostic!(
        "SCOPE MEMBER LOWER label={} unit=true observed={observed} report={report:#?} outcome={:?}",
        c.label,
        lowered.as_ref().map(|(_, e)| e)
    );
    need(
        observed,
        "P24 original same-analysis callback was reached",
        c.label,
    )?;
    let (program, entry) = lowered?;
    let checked = proof
        .as_ref()
        .ok_or_else(|| failure("P24 same-analysis original facts", &report))
        .and_then(|proof| graph::check(c, &parsed, &input, proof, &program, entry));
    report.query(
        "P24 actual receiver/caller owner/full provider incoming and cleanup",
        &checked,
    );
    let verified = crate::ssa::verify::verify_program(&program).map_err(|e| failure("P24 SSA", e));
    report.query("P24 full original SSA verifier", &verified);
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .map(|_| ())
        .map_err(|e| failure("P24 LLVM render/verify", e));
    report.query("P24 full original LLVM render/verify", &llvm);
    diagnostic!(
        "SCOPE MEMBER FINAL label={} unit=true report={report:#?}",
        c.label
    );
    if report.passed() {
        Ok(())
    } else {
        Err(failure("P24 original positive regression", report))
    }
}
#[test]
fn unit_original_member_map_parameter_preserves_full_identity() {
    let outcomes = sources()
        .into_iter()
        .map(|c| {
            let result = observe(&c);
            diagnostic!(
                "SCOPE MEMBER CASE label={} unit=true result={result:?}",
                c.label
            );
            (c.label, result)
        })
        .collect::<Vec<_>>();
    assert_eq!(outcomes.len(), 2);
    assert_diagnostic!(outcomes.iter().all(|(_, r)| r.is_ok()), "{outcomes:#?}");
}
