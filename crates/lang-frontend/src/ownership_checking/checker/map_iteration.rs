//! Map source 一次求值；Key/Value 的两个 Shared loan 共同依赖真实 source lease。
use super::loan::{ActiveLoan, ActiveLoanOwner, ActiveLoanTarget};
use super::{AccessKind, Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolKind,
    ownership_checking::{
        IterationSource, LoanKind, LoanTarget, MapIterationComponent, MapIterationLoan,
        MapIterationOwnershipPlan, MapIterationSourceAccess, OwnershipBindingDescriptor,
        OwnershipBindingKind, OwnershipPlace,
    },
    parser::Expression,
    type_checking::{
        AggregateProjectionKind, AggregateProjectionReceiver, ExpressionCategory,
        MapIterationDescriptor, ParameterMode,
    },
};

impl Checker<'_> {
    pub(super) fn map_iteration_inputs_valid(&self) -> bool {
        self.parsed.diagnostics().is_empty()
            && self.names.diagnostics().is_empty()
            && self.typed.diagnostics().is_empty()
    }

    pub(super) fn check_map_iteration(
        &mut self,
        descriptor: MapIterationDescriptor,
        body: StatementId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let statement = descriptor.statement();
        let source = descriptor.source();
        let source_span = self.parsed.ast().expressions().get(source)?.span();
        let errors = self.diagnostics.len();
        let mut prefix = self.check_map_source(source, state)?;
        let Some(next) = prefix.next.as_mut() else {
            return Ok(prefix);
        };
        if self.diagnostics.len() != errors {
            return self.check_maybe_loop(prefix, body, errors);
        }
        let place = self.place(source)?;
        let (origin, source_access) = if let Some(place) = place {
            let source_access = if next.non_owning.contains_key(&place.root()) {
                MapIterationSourceAccess::Shared
            } else {
                let mode = self.typed.parameter_mode(place.root()).or_else(|| {
                    self.names
                        .symbols()
                        .get(place.root().index())
                        .filter(|symbol| symbol.kind() == SymbolKind::Field)
                        .and(self.current_receiver_mode)
                });
                match mode {
                    Some(ParameterMode::Borrow) => MapIterationSourceAccess::Shared,
                    Some(ParameterMode::Inout) => MapIterationSourceAccess::Exclusive,
                    _ => MapIterationSourceAccess::Owned,
                }
            };
            let origin = self.canonical_borrow_place(&place, next);
            if !self.access_place(&origin, AccessKind::SharedLoan, false, source_span, next)? {
                return self.check_maybe_loop(prefix, body, errors);
            }
            next.loans.push(ActiveLoan {
                owner: ActiveLoanOwner::IterationSource(statement),
                target: ActiveLoanTarget::Place(origin.clone()),
                kind: LoanKind::Shared,
                origin: source_span,
            });
            (LoanTarget::Place(origin), source_access)
        } else {
            (
                LoanTarget::Temporary(self.map_source_temporary_owner(source)?.ok_or(
                    OwnershipCheckingError::InvalidMapIteration {
                        source_unit: None,
                        statement: statement.index(),
                    },
                )?),
                MapIterationSourceAccess::Temporary,
            )
        };
        let symbols = descriptor.bindings();
        let loans = [MapIterationComponent::Key, MapIterationComponent::Value].map(|component| {
            let symbol = match component {
                MapIterationComponent::Key => symbols[0],
                MapIterationComponent::Value => symbols[1],
            };
            let declaration = self.names.symbols()[symbol.index()].span();
            next.moved.remove(&symbol);
            next.non_owning.insert(symbol, declaration);
            // Runtime identity resolves through the same provider/source fact as the
            // published loan. The target is the non-owning binding's access path.
            next.loans.push(ActiveLoan {
                owner: ActiveLoanOwner::MapIterationBinding {
                    statement,
                    source,
                    component,
                },
                target: ActiveLoanTarget::Place(OwnershipPlace::new(symbol, Vec::new())),
                kind: LoanKind::Shared,
                origin: declaration,
            });
            MapIterationLoan {
                statement,
                source,
                component,
                binding: symbol,
                origin: origin.clone(),
            }
        });
        self.map_iterations.insert(
            statement.index(),
            MapIterationOwnershipPlan {
                descriptor,
                source: IterationSource::BorrowedSource(origin),
                source_access,
                bindings: symbols.map(|symbol| {
                    OwnershipBindingDescriptor::new(symbol, OwnershipBindingKind::Shared)
                }),
                loans,
                exits: Vec::new(),
                closure_flow: Default::default(),
                capture_graph: Default::default(),
                closure_phis: Vec::new(),
                closure_phi_incomings: Vec::new(),
            },
        );
        self.check_iteration_body(statement, body, prefix, errors, &symbols, source_span)
    }

    /// 已解析字段读取没有额外求值；只沿真实 receiver 递归，不把 temporary 投影误作 owner。
    fn check_map_source(
        &mut self,
        expression: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        if let Some(receiver) = self.map_source_projection_receiver(expression)? {
            self.check_map_source(receiver, state)
        } else {
            self.check_expression(expression, state, ExpressionUse::Place)
        }
    }

    pub(super) fn map_source_projection_receiver(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
        if let Expression::Group { expression } =
            self.parsed.ast().expressions().get(expression)?.payload()
        {
            return Ok(Some(*expression));
        }
        if self.place(expression)?.is_some() {
            return Ok(None);
        }
        Ok(self
            .typed
            .aggregate_projection(expression)
            .and_then(|projection| {
                if projection.kind() != AggregateProjectionKind::Field {
                    return None;
                }
                match projection.receiver() {
                    AggregateProjectionReceiver::Expression(receiver) => Some(receiver),
                    _ => None,
                }
            }))
    }

    pub(super) fn map_source_temporary_owner(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
        if let Some(receiver) = self.map_source_projection_receiver(expression)? {
            return self.map_source_temporary_owner(receiver);
        }
        if let Some(owner) = self.temporary_element_owner(expression)? {
            return Ok(Some(owner));
        }
        Ok(
            (self.typed.expression_category(expression) == Some(ExpressionCategory::Temporary)
                && self.borrow_result_call(expression).is_none())
            .then_some(expression),
        )
    }
}
