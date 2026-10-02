//! Narrow source bridge for zero-argument Unit `move` closures owning String captures.

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{NameResolution, SymbolId},
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource, OwnershipCheckedFile,
    },
    parser::{Expression, ParsedFile},
    source::Span,
    type_checking::{BuiltinType, TypeKind, TypedFile},
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error,
    nominal::NominalTypeMapper, value,
};
use crate::ssa::model::{
    ClosureCaptureMode as SsaCaptureMode, ClosureCaptureOperand, ClosureCaptureType, EntityId,
    EntityType, FunctionId, LoanKind, Module, Operation, Origin, SsaTypeId,
};

#[derive(Clone, Copy)]
pub(super) struct CapturePlan {
    pub(super) symbol: SymbolId,
    pub(super) ty: SsaTypeId,
    pub(super) span: Span,
}

#[derive(Clone)]
pub(super) struct ClosurePlan {
    pub(super) closure: SsaTypeId,
    pub(super) thunk: FunctionId,
    pub(super) body: StatementId,
    pub(super) span: Span,
    pub(super) captures: Vec<CapturePlan>,
}

pub(super) fn declare(
    module: &mut Module,
    mapper: &mut NominalTypeMapper,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<BTreeMap<usize, ClosurePlan>, LoweringError> {
    let mut plans = BTreeMap::new();
    for (expression, node) in parsed.ast().expressions().iter() {
        let Expression::Lambda {
            move_span,
            parameters,
            body,
            ..
        } = node.payload()
        else {
            continue;
        };
        // Resource capture/instance cleanup inside closures has no supported native recipe
        // in this slice. Keep it explicit instead of publishing plain closure glue.
        if parsed
            .ast()
            .expressions()
            .iter()
            .any(|(expression, child)| {
                node.span().start() <= child.span().start()
                    && child.span().end() <= node.span().end()
                    && typed
                        .expression_type(expression)
                        .is_some_and(|ty| typed.is_resource_type(ty) == Some(true))
            })
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
        let Some(TypeKind::Function {
            parameters: callable_parameters,
            return_type,
            ..
        }) = typed
            .expression_type(expression)
            .and_then(|ty| typed.types().get(ty))
        else {
            return Err(error(LoweringErrorKind::MissingFact, node.span()));
        };
        if move_span.is_none()
            || !parameters.is_empty()
            || !callable_parameters.is_empty()
            || typed.types().get(*return_type) != Some(&TypeKind::Builtin(BuiltinType::Unit))
            || !owned
                .closure(expression)
                .is_some_and(|closure| closure.move_owned())
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
        let mut captures = Vec::new();
        for capture in owned.captures_of(expression) {
            let ClosureCaptureSource::Symbol(symbol) = capture.source() else {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    capture.reference_span(),
                ));
            };
            if capture.mode() != ClosureCaptureMode::Owned
                || capture.effect() != ClosureCaptureEffect::Move
                || typed.types().get(capture.ty()) != Some(&TypeKind::Builtin(BuiltinType::String))
            {
                return Err(error(
                    LoweringErrorKind::UnsupportedNode,
                    capture.reference_span(),
                ));
            }
            captures.push(CapturePlan {
                symbol,
                ty: mapper.intern(module, names, typed, capture.ty(), capture.reference_span())?,
                span: capture.reference_span(),
            });
        }
        if captures.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
        let environment = module
            .add_aggregate_type(
                format!("lambda{}.environment", expression.index()),
                captures.iter().map(|capture| capture.ty).collect(),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, node.span()))?;
        let closure = module
            .add_concrete_closure_type(
                format!("lambda{}.closure", expression.index()),
                Vec::new(),
                Vec::new(),
                environment,
                captures
                    .iter()
                    .map(|capture| ClosureCaptureType {
                        mode: SsaCaptureMode::Owned,
                        ty: capture.ty,
                    })
                    .collect(),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, node.span()))?;
        let thunk = module
            .add_function(
                format!("lambda{}.thunk", expression.index()),
                Vec::new(),
                Origin::Source(node.span()),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, node.span()))?;
        module
            .function_mut(thunk)
            .expect("new thunk exists")
            .add_block(
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: environment,
                }],
                Origin::Source(node.span()),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, node.span()))?;
        plans.insert(
            expression.index(),
            ClosurePlan {
                closure,
                thunk,
                body: *body,
                span: node.span(),
                captures,
            },
        );
    }
    Ok(plans)
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_source_closure(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let plan = self
            .source_closures
            .get(&expression.index())
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        let captures = plan
            .captures
            .iter()
            .map(|capture| match self.bindings.remove(&capture.symbol) {
                Some(LoweredValue::Value(owner)) => Ok(ClosureCaptureOperand::Owned(owner)),
                _ => Err(error(LoweringErrorKind::MissingFact, capture.span)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (_, results) = self.append(
            Operation::ClosureConstruct {
                closure: plan.closure,
                thunk: plan.thunk,
                captures,
            },
            vec![EntityType::Value(plan.closure)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    pub(super) fn bind_capture_views(&mut self, plan: &ClosurePlan) -> Result<(), LoweringError> {
        let [EntityId::Loan(environment)] = self
            .function
            .block(self.block)
            .expect("thunk entry exists")
            .parameters
            .as_slice()
        else {
            return Err(error(LoweringErrorKind::InvalidModel, plan.span));
        };
        let environment = *environment;
        for (field, capture) in plan.captures.iter().enumerate() {
            let (_, results) = self.append(
                Operation::SharedFieldLoan {
                    base: environment,
                    field,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: capture.ty,
                }],
                capture.span,
            )?;
            let EntityId::Loan(loan) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, capture.span));
            };
            self.borrow_bindings.insert(capture.symbol, loan);
        }
        Ok(())
    }
}
