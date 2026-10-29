//! Subset rules for the closure family (SPEC-0364, ADR-0034): Borrow function parameters
//! are callable views; a call through a view parameter, a view forwarded to a Borrow
//! function parameter, and a lambda literal passed to one. Lambda bodies are thunks.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode},
    parser::Expression,
    source::Span,
    type_checking::{Copyability, ParameterMode, UnitCallTarget, UnitTypeId, UnitTypeKind},
};

use super::Checker;
use crate::ssa::refine::RefineCapture;

impl Checker<'_, '_> {
    /// A concrete function type whose callable view target is interned.
    pub(super) fn view_function(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Function { .. }))
            && self.inputs.views.contains_key(&ty)
    }

    /// A Borrow parameter of function type: a callable view.
    pub(super) fn view_parameter(&self, symbol: UnitSymbolId) -> bool {
        self.parameter_mode(symbol) == Some(ParameterMode::Borrow)
            && self
                .symbol_type(symbol)
                .is_some_and(|ty| self.view_function(ty))
    }

    /// `f(...)` where `f` names a view parameter.
    pub(super) fn view_callee(&mut self, call: ExpressionId) -> bool {
        let id = self.id(call);
        let Some(descriptor) = self.inputs.typed.call(id) else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(call) else {
            return false;
        };
        let Expression::Call { callee, .. } = node.payload() else {
            return false;
        };
        let callee = self.ungrouped(*callee);
        let named = matches!(
            self.inputs
                .parsed
                .ast()
                .expressions()
                .get(callee)
                .map(|node| node.payload()),
            Ok(Expression::Name)
        );
        let supported = descriptor.target() == UnitCallTarget::FunctionValue
            && descriptor.receiver().is_none()
            && named
            && self
                .reference(callee)
                .is_some_and(|symbol| self.known.contains(&symbol) && self.view_parameter(symbol));
        self.migrated |= supported;
        supported
    }

    /// The operand of a Borrow function parameter: a view parameter passed on, or a lambda
    /// literal of this instance whose captures the formation supports.
    pub(super) fn callable_argument(&mut self, value: ExpressionId) -> bool {
        let root = self.ungrouped(value);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        let supported = match node.payload() {
            Expression::Name => self
                .reference(root)
                .is_some_and(|symbol| self.known.contains(&symbol) && self.view_parameter(symbol)),
            Expression::Lambda { .. } => {
                self.inputs
                    .callables
                    .get(&self.id(root))
                    .is_some_and(|callable| {
                        callable
                            .captures
                            .iter()
                            .all(|capture| self.capture_supported(capture))
                    })
            }
            _ => false,
        };
        self.migrated |= supported;
        supported
    }

    /// Shared captures borrow a known binding; owned captures copy a Copyable one or move
    /// an owned one.
    fn capture_supported(&self, capture: &RefineCapture) -> bool {
        let symbol = capture.symbol;
        let Some(ty) = self.symbol_type(symbol) else {
            return false;
        };
        if !self.known.contains(&symbol) || !self.storage_type(ty) {
            return false;
        }
        let mode = self.parameter_mode(symbol);
        match (capture.mode, capture.effect) {
            (ClosureCaptureMode::Shared, _) => mode != Some(ParameterMode::Inout),
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Copy) => {
                self.copyability_of(ty) == Copyability::Copyable
                    && mode != Some(ParameterMode::Inout)
            }
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move) => {
                matches!(mode, None | Some(ParameterMode::Value))
            }
            _ => false,
        }
    }

    /// Facts inside a lambda body belong to its thunk, not to this function's body.
    pub(super) fn inside_lambda(&self, origin: Span) -> bool {
        self.inputs.callables.keys().any(|lambda| {
            self.inputs
                .parsed
                .ast()
                .expressions()
                .get(lambda.expression())
                .is_ok_and(|node| {
                    let span = node.span();
                    origin != span
                        && origin.source_id() == span.source_id()
                        && span.start() <= origin.start()
                        && origin.end() <= span.end()
                })
        })
    }
}
