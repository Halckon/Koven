//! Source-qualified body-local call ownership dataflow.

mod closure;
mod construction;
mod container;
mod control;
mod drop_planner;
mod flow;
mod liveness;
mod non_null_assertion;
mod places;
mod rc;
mod receiver;
mod short_circuit;
mod traversal;

use std::collections::BTreeMap;

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, DiagnosticCode, Severity, codes},
    name_resolution::{
        DeclarationId, SourceUnitId, SourceUnitInput, SymbolKind, UnitReferenceTarget,
        UnitSymbolId, ValidatedCompilationUnitNames, ordered_unit_diagnostics,
    },
    parser::{AssignmentOperator, NameMarker, ParsedFile, VariableKind},
    source::{SourceMap, Span},
    type_checking::{
        CompilationUnitTypes, Copyability, ExpressionCategory, ParameterMode, UnitCallableTarget,
        UnitConstructionDescriptor, UnitExpressionId, UnitTypeId,
    },
};

use super::{
    LoanKind, OwnershipBindingKind, OwnershipCheckingError, Transferability,
    UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind,
    UnitCallReceiverOwnershipContract, UnitClosureCaptureDescriptor, UnitClosureDescriptor,
    UnitConditionalReceiverDeliveryFact, UnitConditionalReceiverDropFact,
    UnitConstructionOwnershipPlan, UnitDropFact, UnitLoanFact, UnitLoanTarget,
    UnitNonNullAssertionOwnershipPlan, UnitOwnershipBindingDescriptor, UnitOwnershipDeferredFact,
    UnitOwnershipPlace, UnitRcOwnershipEffect, UnitReceiverOwnershipFact,
    UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget, UnitValueDeliveryFact,
};
use flow::{ActiveLoan, ActiveLoanOwner, ActiveLoanTarget, Flows, State, merge_state};

pub(super) struct Analysis {
    pub(super) short_circuits: Option<Vec<super::constant::UnitShortCircuitPlan>>,
    pub(super) constant_materializations: Vec<super::constant::UnitConstantMaterializationPlan>,
    pub(super) diagnostics: Vec<Diagnostic>,
    pub(super) loans: Vec<UnitLoanFact>,
    pub(super) value_deliveries: Vec<UnitValueDeliveryFact>,
    pub(super) receiver_facts: Vec<UnitReceiverOwnershipFact>,
    pub(super) conditional_receiver_deliveries: Vec<UnitConditionalReceiverDeliveryFact>,
    pub(super) rc_effects: Vec<UnitRcOwnershipEffect>,
    pub(super) construction_plans: Vec<UnitConstructionOwnershipPlan>,
    pub(super) non_null_assertions: Vec<UnitNonNullAssertionOwnershipPlan>,
    pub(super) drops: Vec<UnitDropFact>,
    pub(super) conditional_receiver_drops: Vec<UnitConditionalReceiverDropFact>,
    pub(super) deferred: Vec<UnitOwnershipDeferredFact>,
}

pub(super) struct ClosureInputs<'a> {
    captures: &'a [UnitClosureCaptureDescriptor],
    closures: &'a [UnitClosureDescriptor],
    transferabilities: &'a [Transferability],
}

pub(super) struct CallInputs<'a> {
    arguments: &'a [UnitCallArgumentOwnershipContract],
    receivers: &'a [UnitCallReceiverOwnershipContract],
}

impl<'a> CallInputs<'a> {
    pub(super) const fn new(
        arguments: &'a [UnitCallArgumentOwnershipContract],
        receivers: &'a [UnitCallReceiverOwnershipContract],
    ) -> Self {
        Self {
            arguments,
            receivers,
        }
    }
}

impl<'a> ClosureInputs<'a> {
    pub(super) const fn new(
        captures: &'a [UnitClosureCaptureDescriptor],
        closures: &'a [UnitClosureDescriptor],
        transferabilities: &'a [Transferability],
    ) -> Self {
        Self {
            captures,
            closures,
            transferabilities,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpressionUse {
    Read,
    Consume { parameter_span: Option<Span> },
    Place { parameter_span: Option<Span> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessKind {
    Read,
    Move,
    Mutation,
    SharedLoan,
    ExclusiveLoan,
}

type ConstructionDescriptors =
    BTreeMap<SourceUnitId, BTreeMap<UnitExpressionId, UnitConstructionDescriptor>>;

#[allow(clippy::too_many_arguments)]
pub(super) fn analyze(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    constant_control: bool,
    bindings: &BTreeMap<UnitSymbolId, UnitOwnershipBindingDescriptor>,
    call_inputs: CallInputs<'_>,
    closure_inputs: ClosureInputs<'_>,
) -> Result<Analysis, OwnershipCheckingError> {
    let construction_descriptors = collect_construction_descriptors(
        typed.constructions(),
        names.names().index().source_units().len(),
    )?;
    for source in names.names().index().source_units() {
        let input = inputs
            .iter()
            .copied()
            .find(|input| input.source_id() == source.source_id())
            .ok_or(OwnershipCheckingError::InvalidUnitSource {
                source_unit: source.id().index(),
            })?;
        if let Some(descriptors) = construction_descriptors.get(&source.id()) {
            construction::validate_constructions(
                input.parsed(),
                source.id(),
                typed,
                descriptors.values(),
            )?;
        }
    }
    let use_after_move_code = codes::catalog()?.resolve(codes::USE_AFTER_MOVE)?;
    let partial_move_code = codes::catalog()?.resolve(codes::PARTIAL_MOVE)?;
    let borrowed_move_code = codes::catalog()?.resolve(codes::MOVE_FROM_BORROWED_BINDING)?;
    let loan_conflict_code = codes::catalog()?.resolve(codes::LOAN_CONFLICT)?;
    let immutable_inout_code = codes::catalog()?.resolve(codes::IMMUTABLE_INOUT_PLACE)?;
    let container_element_move_code =
        codes::catalog()?.resolve(codes::MOVE_FROM_CONTAINER_ELEMENT)?;
    let borrowed_closure_escape_code = codes::catalog()?.resolve(codes::BORROWED_CLOSURE_ESCAPE)?;
    let illegal_owned_capture_code = codes::catalog()?.resolve(codes::ILLEGAL_OWNED_CAPTURE)?;
    let non_transferable_delivery_code =
        codes::catalog()?.resolve(codes::NON_TRANSFERABLE_DELIVERY)?;
    let mut diagnostics = Vec::new();
    let mut constant_materializations = Vec::new();
    let mut short_circuits = Vec::new();
    let mut loans = Vec::new();
    let mut value_deliveries = Vec::new();
    let mut receiver_facts = Vec::new();
    let mut conditional_receiver_deliveries = Vec::new();
    let mut rc_effects = Vec::new();
    let mut construction_plans = Vec::new();
    let mut non_null_assertions = Vec::new();
    let mut drops = Vec::new();
    let mut conditional_receiver_drops = Vec::new();
    let mut deferred = Vec::new();
    let empty_construction_descriptors = BTreeMap::new();

    for source in names.names().index().source_units() {
        let input = inputs
            .iter()
            .copied()
            .find(|input| input.source_id() == source.source_id())
            .ok_or(OwnershipCheckingError::InvalidUnitSource {
                source_unit: source.id().index(),
            })?;
        let mut checker = Checker::new(
            sources,
            input.parsed(),
            source.id(),
            names,
            typed,
            bindings,
            call_inputs.arguments,
            call_inputs.receivers,
            closure_inputs.captures,
            closure_inputs.closures,
            closure_inputs.transferabilities,
            construction_descriptors
                .get(&source.id())
                .unwrap_or(&empty_construction_descriptors),
            Codes {
                use_after_move: use_after_move_code,
                partial_move: partial_move_code,
                borrowed_move: borrowed_move_code,
                loan_conflict: loan_conflict_code,
                immutable_inout: immutable_inout_code,
                container_element_move: container_element_move_code,
                borrowed_closure_escape: borrowed_closure_escape_code,
                illegal_owned_capture: illegal_owned_capture_code,
                non_transferable_delivery: non_transferable_delivery_code,
            },
            &mut diagnostics,
            &mut loans,
            &mut value_deliveries,
            &mut receiver_facts,
            &mut conditional_receiver_deliveries,
            &mut rc_effects,
            &mut construction_plans,
            &mut non_null_assertions,
        )?;
        checker.constant_control = constant_control;
        let drop_analysis = checker.run()?;
        short_circuits.extend(checker.short_circuits.into_values());
        constant_materializations.extend(checker.constant_materializations.into_values());
        drops.extend(drop_analysis.drops);
        conditional_receiver_drops.extend(drop_analysis.conditional_receiver_drops);
        deferred.extend(drop_analysis.deferred);
    }

    let diagnostics =
        ordered_unit_diagnostics(sources, names.names().index().source_units(), &diagnostics)?
            .into_iter()
            .cloned()
            .collect();
    if !deferred.is_empty() {
        conditional_receiver_deliveries.clear();
    }
    non_null_assertions.sort_by_key(|plan| plan.descriptor().expression());
    non_null_assertions.dedup_by_key(|plan| plan.descriptor().expression());
    constant_materializations.sort_by_key(|plan| plan.descriptor.expression());
    short_circuits.sort_by_key(|plan| plan.expression);
    Ok(Analysis {
        short_circuits: constant_control.then_some(short_circuits),
        constant_materializations,
        non_null_assertions,
        diagnostics,
        loans,
        value_deliveries,
        receiver_facts,
        conditional_receiver_deliveries,
        rc_effects,
        construction_plans,
        drops,
        conditional_receiver_drops,
        deferred,
    })
}

fn collect_construction_descriptors(
    descriptors: &[UnitConstructionDescriptor],
    source_count: usize,
) -> Result<ConstructionDescriptors, OwnershipCheckingError> {
    let mut by_source = ConstructionDescriptors::new();
    for descriptor in descriptors {
        let expression = descriptor.expression();
        if expression.source_unit().index() >= source_count {
            return Err(OwnershipCheckingError::InvalidUnitConstruction {
                source_unit: expression.source_unit().index(),
                expression: expression.expression().index(),
            });
        }
        if by_source
            .entry(expression.source_unit())
            .or_default()
            .insert(expression, descriptor.clone())
            .is_some()
        {
            return Err(OwnershipCheckingError::InvalidUnitConstruction {
                source_unit: expression.source_unit().index(),
                expression: expression.expression().index(),
            });
        }
    }
    Ok(by_source)
}

#[derive(Clone, Copy)]
struct Codes {
    use_after_move: DiagnosticCode,
    partial_move: DiagnosticCode,
    borrowed_move: DiagnosticCode,
    loan_conflict: DiagnosticCode,
    immutable_inout: DiagnosticCode,
    container_element_move: DiagnosticCode,
    borrowed_closure_escape: DiagnosticCode,
    illegal_owned_capture: DiagnosticCode,
    non_transferable_delivery: DiagnosticCode,
}

#[derive(Clone, Copy)]
struct ReceiverContext {
    owner: DeclarationId,
    mode: ParameterMode,
    ty: UnitTypeId,
    declaration_span: Span,
}

#[allow(clippy::too_many_arguments)]
struct Checker<'a> {
    constant_control: bool,
    short_circuits: BTreeMap<UnitExpressionId, super::constant::UnitShortCircuitPlan>,
    constant_materializations:
        BTreeMap<UnitExpressionId, super::constant::UnitConstantMaterializationPlan>,
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
    source_unit: SourceUnitId,
    names: &'a ValidatedCompilationUnitNames,
    typed: &'a CompilationUnitTypes,
    bindings: &'a BTreeMap<UnitSymbolId, UnitOwnershipBindingDescriptor>,
    captures: &'a [UnitClosureCaptureDescriptor],
    closures: &'a [UnitClosureDescriptor],
    transferabilities: &'a [Transferability],
    construction_descriptors: &'a BTreeMap<UnitExpressionId, UnitConstructionDescriptor>,
    references_by_span: BTreeMap<(usize, usize), UnitSymbolId>,
    symbols_by_span: BTreeMap<(usize, usize), UnitSymbolId>,
    variable_kinds: BTreeMap<UnitSymbolId, VariableKind>,
    field_kinds: BTreeMap<UnitSymbolId, VariableKind>,
    contracts_by_call: BTreeMap<UnitExpressionId, Vec<UnitCallArgumentOwnershipContract>>,
    receiver_contracts_by_call: BTreeMap<UnitExpressionId, UnitCallReceiverOwnershipContract>,
    current_receiver: Option<ReceiverContext>,
    expression_live_after: Vec<std::collections::BTreeSet<UnitSymbolId>>,
    statement_live_after: Vec<std::collections::BTreeSet<UnitSymbolId>>,
    codes: Codes,
    diagnostics: &'a mut Vec<Diagnostic>,
    loans: &'a mut Vec<UnitLoanFact>,
    value_deliveries: &'a mut Vec<UnitValueDeliveryFact>,
    receiver_facts: &'a mut Vec<UnitReceiverOwnershipFact>,
    conditional_receiver_deliveries: &'a mut Vec<UnitConditionalReceiverDeliveryFact>,
    rc_effects: &'a mut Vec<UnitRcOwnershipEffect>,
    construction_plans: &'a mut Vec<UnitConstructionOwnershipPlan>,
    non_null_assertions: &'a mut Vec<UnitNonNullAssertionOwnershipPlan>,
}

impl<'a> Checker<'a> {
    /// Only Phase 2 selected uses are values; their namespace is never evaluated.
    fn is_constant_use(&self, expression: ExpressionId) -> bool {
        self.constant_use(expression).is_some()
    }

    fn constant_use(
        &self,
        expression: ExpressionId,
    ) -> Option<&crate::type_checking::UnitConstantUseDescriptor> {
        let uses = self.typed.constants()?.uses();
        uses.binary_search_by_key(&self.unit_expression(expression), |usage| {
            usage.expression()
        })
        .ok()
        .map(|index| &uses[index])
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        sources: &'a SourceMap,
        parsed: &'a ParsedFile,
        source_unit: SourceUnitId,
        names: &'a ValidatedCompilationUnitNames,
        typed: &'a CompilationUnitTypes,
        bindings: &'a BTreeMap<UnitSymbolId, UnitOwnershipBindingDescriptor>,
        contracts: &[UnitCallArgumentOwnershipContract],
        receiver_contracts: &[UnitCallReceiverOwnershipContract],
        captures: &'a [UnitClosureCaptureDescriptor],
        closures: &'a [UnitClosureDescriptor],
        transferabilities: &'a [Transferability],
        construction_descriptors: &'a BTreeMap<UnitExpressionId, UnitConstructionDescriptor>,
        codes: Codes,
        diagnostics: &'a mut Vec<Diagnostic>,
        loans: &'a mut Vec<UnitLoanFact>,
        value_deliveries: &'a mut Vec<UnitValueDeliveryFact>,
        receiver_facts: &'a mut Vec<UnitReceiverOwnershipFact>,
        conditional_receiver_deliveries: &'a mut Vec<UnitConditionalReceiverDeliveryFact>,
        rc_effects: &'a mut Vec<UnitRcOwnershipEffect>,
        construction_plans: &'a mut Vec<UnitConstructionOwnershipPlan>,
        non_null_assertions: &'a mut Vec<UnitNonNullAssertionOwnershipPlan>,
    ) -> Result<Self, OwnershipCheckingError> {
        sources.source_text(parsed.source_id())?;
        let resolution = names
            .names()
            .source_units()
            .get(source_unit.index())
            .filter(|source| source.source_unit() == source_unit)
            .ok_or(OwnershipCheckingError::InvalidUnitSource {
                source_unit: source_unit.index(),
            })?
            .resolution();
        let mut symbols_by_span = BTreeMap::new();
        for symbol in resolution.symbols().iter() {
            symbols_by_span.insert(
                span_key(symbol.span()),
                UnitSymbolId::new(source_unit, symbol.id()),
            );
        }
        let mut references_by_span = BTreeMap::new();
        for reference in names
            .names()
            .references()
            .iter()
            .filter(|reference| reference.source_unit() == source_unit)
        {
            let symbol = match reference.target() {
                UnitReferenceTarget::Symbol(symbol) => Some(*symbol),
                UnitReferenceTarget::Declaration(declaration) => {
                    names.names().declaration_symbol(*declaration)
                }
                _ => None,
            };
            if let Some(symbol) = symbol {
                references_by_span.insert(span_key(reference.span()), symbol);
            }
        }
        let mut contracts_by_call = BTreeMap::<_, Vec<_>>::new();
        for contract in contracts
            .iter()
            .copied()
            .filter(|contract| contract.call().source_unit() == source_unit)
        {
            contracts_by_call
                .entry(contract.call())
                .or_default()
                .push(contract);
        }
        let mut receiver_contracts_by_call = BTreeMap::new();
        for contract in receiver_contracts
            .iter()
            .copied()
            .filter(|contract| contract.call().source_unit() == source_unit)
        {
            if receiver_contracts_by_call
                .insert(contract.call(), contract)
                .is_some()
            {
                return Err(OwnershipCheckingError::InvalidUnitCall {
                    source_unit: contract.call().source_unit().index(),
                    expression: contract.call().expression().index(),
                });
            }
        }
        Ok(Self {
            constant_control: false,
            short_circuits: BTreeMap::new(),
            constant_materializations: BTreeMap::new(),
            sources,
            parsed,
            source_unit,
            names,
            typed,
            bindings,
            captures,
            closures,
            transferabilities,
            construction_descriptors,
            references_by_span,
            symbols_by_span,
            variable_kinds: BTreeMap::new(),
            field_kinds: BTreeMap::new(),
            contracts_by_call,
            receiver_contracts_by_call,
            current_receiver: None,
            expression_live_after: Vec::new(),
            statement_live_after: Vec::new(),
            codes,
            diagnostics,
            loans,
            value_deliveries,
            receiver_facts,
            conditional_receiver_deliveries,
            rc_effects,
            construction_plans,
            non_null_assertions,
        })
    }

    fn receiver_context(&self, name: NameMarker) -> Option<ReceiverContext> {
        let symbol = self.marker_symbol(name).copied()?;
        self.typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|declaration| declaration.nominal())
            .find_map(|nominal| {
                nominal.members().iter().find_map(|callable| {
                    (callable.target() == UnitCallableTarget::Symbol(symbol))
                        .then(|| callable.receiver())
                        .flatten()
                        .map(|receiver| ReceiverContext {
                            owner: nominal.declaration(),
                            mode: receiver.mode(),
                            ty: receiver.ty(),
                            declaration_span: receiver.declaration_span(),
                        })
                })
            })
    }

    fn run(&mut self) -> Result<drop_planner::Analysis, OwnershipCheckingError> {
        for (item, _) in self.parsed.ast().items().iter() {
            self.collect_mutability(item)?;
        }
        let liveness = liveness::build(self)?;
        self.expression_live_after = liveness.expression_after;
        self.statement_live_after = liveness.statement_after;
        for &root in self.parsed.roots() {
            self.check_item(root, State::default())?;
        }
        if self.diagnostics.is_empty() {
            drop_planner::plan(self)
        } else {
            Ok(drop_planner::Analysis::default())
        }
    }

    fn apply_contract(
        &mut self,
        contract: UnitCallArgumentOwnershipContract,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        match contract.kind() {
            UnitCallArgumentOwnershipKind::Value => {
                let Some((kind, source)) = self.value_delivery(contract, state)? else {
                    return Ok(());
                };
                self.value_deliveries.push(UnitValueDeliveryFact::new(
                    contract.call(),
                    contract.argument(),
                    source,
                    kind,
                    contract.argument_span(),
                    contract.parameter_span(),
                ));
            }
            UnitCallArgumentOwnershipKind::SharedLoan
            | UnitCallArgumentOwnershipKind::ExclusiveLoan => {
                let kind = if contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan {
                    LoanKind::Shared
                } else {
                    LoanKind::Exclusive
                };
                if kind == LoanKind::Exclusive
                    && !self.is_mutable_place(contract.argument().expression())?
                {
                    let mut diagnostic = Diagnostic::new(
                        self.sources,
                        Severity::Error,
                        self.codes.immutable_inout,
                        "inout argument is not a mutable place",
                        contract.loan_begin_span(),
                    )?;
                    if let Some(place) = self.place(contract.argument().expression())? {
                        diagnostic.add_label(
                            self.sources,
                            self.symbol_span(place.root())?,
                            "immutable binding declared here",
                        )?;
                    }
                    add_parameter_label(self.sources, &mut diagnostic, contract.parameter_span())?;
                    self.diagnostics.push(diagnostic);
                    return Ok(());
                }
                let target = match contract.category() {
                    ExpressionCategory::Temporary => {
                        if kind == LoanKind::Exclusive {
                            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                                source_unit: contract.argument().source_unit().index(),
                                expression: contract.argument().expression().index(),
                            });
                        }
                        UnitLoanTarget::Temporary(
                            self.constant_temporary_origin(contract.argument().expression())
                                .map_or(contract.argument(), |(owner, _)| owner),
                        )
                    }
                    ExpressionCategory::Place => {
                        if let Some(place) = self.loan_place(contract.argument().expression())? {
                            let access = if kind == LoanKind::Shared {
                                AccessKind::SharedLoan
                            } else {
                                AccessKind::ExclusiveLoan
                            };
                            if !self.access_place(
                                &place,
                                access,
                                contract.loan_begin_span(),
                                contract.parameter_span(),
                                state,
                            )? {
                                return Ok(());
                            }
                            state.loans.push(ActiveLoan {
                                owner: ActiveLoanOwner::Call(contract.call()),
                                target: ActiveLoanTarget::Place(place.clone()),
                                kind,
                                origin: contract.loan_begin_span(),
                            });
                            UnitLoanTarget::Place(place)
                        } else if let Some(owner) =
                            self.temporary_projection_owner(contract.argument().expression())?
                        {
                            UnitLoanTarget::Temporary(owner)
                        } else {
                            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                                source_unit: contract.argument().source_unit().index(),
                                expression: contract.argument().expression().index(),
                            });
                        }
                    }
                };
                self.loans.push(UnitLoanFact::new(
                    contract.call(),
                    contract.argument(),
                    target,
                    kind,
                    contract.loan_begin_span(),
                    contract.call_span(),
                    contract.parameter_span(),
                ));
            }
        }
        Ok(())
    }

    fn check_assignment(
        &mut self,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let entry_state = state.clone();
        let diagnostic_count = self.diagnostics.len();
        let fact_lengths = (
            self.loans.len(),
            self.value_deliveries.len(),
            self.receiver_facts.len(),
            self.rc_effects.len(),
            self.construction_plans.len(),
            self.non_null_assertions.len(),
        );
        let mut flows = self.check_expression(
            value,
            state,
            ExpressionUse::Consume {
                parameter_span: None,
            },
        )?;
        if self.diagnostics.len() != diagnostic_count {
            self.rollback_facts(fact_lengths);
            return Ok(Self::restore_flow_states(flows, &entry_state));
        }
        let target_span = self.parsed.ast().expressions().get(target)?.span();
        let place = self.place(target)?;
        if let Some(place) = &place
            && !self.is_mutable_place(target)?
        {
            let declaration = place.fields().last().copied().unwrap_or(place.root());
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.immutable_inout,
                "assignment target is not a mutable place",
                target_span,
            )?;
            diagnostic.add_label(
                self.sources,
                self.symbol_span(declaration)?,
                "immutable binding declared here",
            )?;
            self.diagnostics.push(diagnostic);
            self.rollback_facts(fact_lengths);
            return Ok(Self::restore_flow_states(flows, &entry_state));
        }
        let Some(state) = flows.next.as_mut() else {
            return Ok(flows);
        };
        if let Some(place) = place {
            if operator != AssignmentOperator::Assign {
                self.access_place(&place, AccessKind::Read, target_span, None, state)?;
            }
            if self.access_place(&place, AccessKind::Mutation, target_span, None, state)?
                && operator == AssignmentOperator::Assign
                && place.fields().is_empty()
            {
                state.moved.remove(&place.root());
            }
            if self.diagnostics.len() != diagnostic_count {
                self.rollback_facts(fact_lengths);
                return Ok(Self::restore_flow_states(flows, &entry_state));
            }
        } else {
            flows = self.chain_expression(flows, target, ExpressionUse::Read)?;
            if self.diagnostics.len() != diagnostic_count {
                self.rollback_facts(fact_lengths);
                return Ok(Self::restore_flow_states(flows, &entry_state));
            }
        }
        Ok(flows)
    }

    fn rollback_facts(&mut self, lengths: (usize, usize, usize, usize, usize, usize)) {
        self.loans.truncate(lengths.0);
        self.value_deliveries.truncate(lengths.1);
        self.receiver_facts.truncate(lengths.2);
        self.rc_effects.truncate(lengths.3);
        self.construction_plans.truncate(lengths.4);
        self.non_null_assertions.truncate(lengths.5);
    }

    fn restore_flow_states(mut flows: Flows, entry: &State) -> Flows {
        for state in [&mut flows.next, &mut flows.breaks, &mut flows.continues] {
            if state.is_some() {
                *state = Some(entry.clone());
            }
        }
        flows
    }

    fn chain_expression(
        &mut self,
        mut flows: Flows,
        id: ExpressionId,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        if let Some(next) = flows.next.take() {
            flows.merge(self.check_expression(id, next, usage)?);
        }
        Ok(flows)
    }

    fn use_name(
        &mut self,
        expression: ExpressionId,
        span: Span,
        usage: ExpressionUse,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(symbol) = self.reference_symbol(span) else {
            return Ok(());
        };
        if !self.is_place_symbol(symbol) {
            return Ok(());
        }
        let place = UnitOwnershipPlace::new(symbol, Vec::new());
        match usage {
            ExpressionUse::Read => {
                self.access_place(&place, AccessKind::Read, span, None, state)?;
            }
            ExpressionUse::Consume { parameter_span } => {
                self.consume_place(&place, expression, span, parameter_span, state)?;
            }
            ExpressionUse::Place { parameter_span } => {
                self.ensure_available(&place, span, parameter_span, state)?;
            }
        }
        Ok(())
    }

    fn consume_place(
        &mut self,
        place: &UnitOwnershipPlace,
        expression: ExpressionId,
        primary: Span,
        parameter_span: Option<Span>,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let ty = self
            .typed
            .expression_type(self.unit_expression(expression))
            .or_else(|| self.typed.symbol_type(place.root()));
        let Some(ty) = ty else {
            return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                source_unit: self.source_unit.index(),
                expression: expression.index(),
            });
        };
        let move_only = match self.typed.copyability(ty) {
            Copyability::Copyable => false,
            Copyability::MoveOnly => true,
            Copyability::Unknown | Copyability::Error => {
                return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                    source_unit: self.source_unit.index(),
                    expression: expression.index(),
                });
            }
        };
        if move_only
            && (self
                .bindings
                .get(&place.root())
                .is_some_and(|binding| binding.kind() != OwnershipBindingKind::Owned)
                || state.non_owning.contains_key(&place.root()))
        {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.borrowed_move,
                "cannot move a non-Copyable value out of a borrowed binding",
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                state
                    .non_owning
                    .get(&place.root())
                    .copied()
                    .or_else(|| {
                        self.bindings
                            .get(&place.root())
                            .map(|binding| binding.declaration_span())
                    })
                    .unwrap_or(primary),
                "non-owning binding established here",
            )?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(());
        }
        let access = if move_only {
            AccessKind::Move
        } else {
            AccessKind::Read
        };
        if !self.access_place(place, access, primary, parameter_span, state)? || !move_only {
            return Ok(());
        }
        if place.element().is_some() {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.container_element_move,
                "cannot move a non-Copyable element out of a sequential container",
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                self.symbol_span(place.root())?,
                "container owner remains responsible for every initialized element",
            )?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(());
        }
        if !place.fields().is_empty() || self.symbol_kind(place.root()) == Some(SymbolKind::Field) {
            let field = place.fields().last().copied().unwrap_or(place.root());
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.partial_move,
                "cannot move a non-Copyable component out of its owner",
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                self.symbol_span(field)?,
                "non-Copyable component declared here",
            )?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(());
        }
        state.moved.insert(place.root(), primary);
        Ok(())
    }

    fn ensure_available(
        &mut self,
        place: &UnitOwnershipPlace,
        primary: Span,
        parameter_span: Option<Span>,
        state: &State,
    ) -> Result<bool, OwnershipCheckingError> {
        let Some(origin) = state.moved.get(&place.root()).copied() else {
            return Ok(true);
        };
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.codes.use_after_move,
            "use of moved value",
            primary,
        )?;
        diagnostic.add_label(self.sources, origin, "value was moved here")?;
        add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
        self.diagnostics.push(diagnostic);
        Ok(false)
    }

    fn access_place(
        &mut self,
        place: &UnitOwnershipPlace,
        access: AccessKind,
        primary: Span,
        parameter_span: Option<Span>,
        state: &mut State,
    ) -> Result<bool, OwnershipCheckingError> {
        if self.symbol_kind(place.root()) == Some(SymbolKind::Field)
            && !self.ensure_this_available_at(primary, parameter_span, state)?
        {
            return Ok(false);
        }
        // A whole-root assignment restores a moved binding without reading its old value.
        let requires_existing_value = access != AccessKind::Mutation || !place.is_root();
        if requires_existing_value
            && !self.ensure_available(place, primary, parameter_span, state)?
        {
            return Ok(false);
        }
        if access == AccessKind::Mutation
            && let Some(origin) = state.immutable_captures.get(&place.root()).copied()
        {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.loan_conflict,
                "captured bindings are immutable in v1 closures",
                primary,
            )?;
            diagnostic.add_label(self.sources, origin, "conflicting loan starts here")?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(false);
        }
        let conflict = state.loans.iter().find(|loan| {
            let overlaps = match &loan.target {
                ActiveLoanTarget::Place(target) => target.overlaps(place),
                ActiveLoanTarget::This => self.symbol_kind(place.root()) == Some(SymbolKind::Field),
            };
            overlaps
                && !matches!(
                    (loan.kind, access),
                    (LoanKind::Shared, AccessKind::Read | AccessKind::SharedLoan)
                )
        });
        if let Some(conflict) = conflict {
            let message = match access {
                AccessKind::Read => "read conflicts with an active exclusive loan",
                AccessKind::Move => "move conflicts with an active loan",
                AccessKind::Mutation => "mutation conflicts with an active loan",
                AccessKind::SharedLoan | AccessKind::ExclusiveLoan => {
                    "new loan conflicts with an active loan"
                }
            };
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.codes.loan_conflict,
                message,
                primary,
            )?;
            diagnostic.add_label(
                self.sources,
                conflict.origin,
                "conflicting loan starts here",
            )?;
            add_parameter_label(self.sources, &mut diagnostic, parameter_span)?;
            self.diagnostics.push(diagnostic);
            return Ok(false);
        }
        if self.symbol_kind(place.root()) == Some(SymbolKind::Field)
            && matches!(access, AccessKind::Mutation | AccessKind::ExclusiveLoan)
            && !self.require_mutable_this(primary, parameter_span)?
        {
            return Ok(false);
        }
        Ok(true)
    }

    fn is_place_symbol(&self, symbol: UnitSymbolId) -> bool {
        matches!(
            self.symbol_kind(symbol),
            Some(
                SymbolKind::Variable
                    | SymbolKind::Field
                    | SymbolKind::ValueParameter
                    | SymbolKind::LambdaParameter
                    | SymbolKind::ForBinding
                    | SymbolKind::DestructuringBinding
            )
        )
    }

    fn is_move_only_variable(&self, symbol: UnitSymbolId) -> bool {
        if !matches!(
            self.symbol_kind(symbol),
            Some(
                SymbolKind::Variable
                    | SymbolKind::ValueParameter
                    | SymbolKind::LambdaParameter
                    | SymbolKind::ForBinding
                    | SymbolKind::DestructuringBinding
            )
        ) {
            return false;
        }
        if self
            .bindings
            .get(&symbol)
            .is_some_and(|binding| binding.kind() != OwnershipBindingKind::Owned)
        {
            return false;
        }
        self.typed
            .symbol_type(symbol)
            .is_some_and(|ty| self.typed.copyability(ty) == Copyability::MoveOnly)
    }

    fn symbol_kind(&self, symbol: UnitSymbolId) -> Option<SymbolKind> {
        self.names
            .names()
            .source_units()
            .get(symbol.source_unit().index())
            .and_then(|source| source.resolution().symbols().get(symbol.symbol().index()))
            .map(|symbol| symbol.kind())
    }

    fn symbol_span(&self, symbol: UnitSymbolId) -> Result<Span, OwnershipCheckingError> {
        self.names
            .names()
            .source_units()
            .get(symbol.source_unit().index())
            .and_then(|source| source.resolution().symbols().get(symbol.symbol().index()))
            .map(|symbol| symbol.span())
            .ok_or(OwnershipCheckingError::InvalidUnitSymbol {
                source_unit: symbol.source_unit().index(),
                symbol: symbol.symbol().index(),
            })
    }

    fn mark_available(&self, marker: NameMarker, state: &mut State) {
        if let Some(symbol) = self.marker_symbol(marker) {
            state.moved.remove(symbol);
        }
    }

    fn marker_symbol(&self, marker: NameMarker) -> Option<&UnitSymbolId> {
        let NameMarker::Present(span) = marker else {
            return None;
        };
        self.symbols_by_span.get(&span_key(span))
    }

    fn reference_symbol(&self, span: Span) -> Option<UnitSymbolId> {
        self.references_by_span.get(&span_key(span)).copied()
    }

    const fn unit_expression(&self, expression: ExpressionId) -> UnitExpressionId {
        UnitExpressionId::new(self.source_unit, expression)
    }
}

fn add_parameter_label(
    sources: &SourceMap,
    diagnostic: &mut Diagnostic,
    parameter_span: Option<Span>,
) -> Result<(), OwnershipCheckingError> {
    if let Some(parameter_span) = parameter_span {
        diagnostic.add_label(sources, parameter_span, "selected parameter declared here")?;
    }
    Ok(())
}

const fn span_key(span: Span) -> (usize, usize) {
    (span.start(), span.end())
}

#[cfg(test)]
mod tests {
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::{ParsedFile, parse_file},
        source::{SourceId, SourceMap},
        type_checking::{check_compilation_unit_types, standard_environments},
    };

    use super::{OwnershipCheckingError, collect_construction_descriptors};

    fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
        let source = sources.add_source(name, text).expect("unique source");
        let lexed = lex(sources, source).expect("lexing succeeds internally");
        let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        (source, parsed)
    }

    #[test]
    fn duplicate_construction_descriptor_is_rejected_before_dataflow() {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(
            &mut sources,
            "p/source.ko",
            "package p\nclass Resource {}\nfun build(): Unit { val result = Resource() }",
        );
        let inputs = [SourceUnitInput::new("root", "p/source.ko", source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .expect("name resolution succeeds internally")
            .validate()
            .expect("valid names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("type checking succeeds internally")
            .validate()
            .expect("valid types");
        let descriptor = typed.types().constructions()[0].clone();

        let error = collect_construction_descriptors(
            &[descriptor.clone(), descriptor],
            names.names().index().source_units().len(),
        )
        .expect_err("duplicate construction locator must fail before body traversal");

        assert!(matches!(
            error,
            OwnershipCheckingError::InvalidUnitConstruction { .. }
        ));
    }
}
