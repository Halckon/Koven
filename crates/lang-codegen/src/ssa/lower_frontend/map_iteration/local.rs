//! The bounded, ordinary named-local Map proof consumes only published FE facts.
//! Formation history is an anchor; entry Expression owners and every incoming
//! establish current liveness. Runtime still checks birth, transport and absence.
mod conditions;
mod identity;
pub(super) use identity::{lambda_body, producer};
mod topology;

use super::*;
use lang_frontend::{
    name_resolution::{Namespace, ReferenceTarget},
    ownership_checking::{
        CleanupConditionId, CleanupOwnerValueId, CleanupSelectorId, DropFact, DropTarget,
        LoanEndPoint, LoanKind,
    },
    parser::NameMarker,
    type_checking::{
        CallableResultSource, CallableTarget, ExpressionCategory, MapIterationDescriptor,
    },
};

pub(super) use self::consumer_prove as consumer;

#[derive(Clone)]
pub(super) struct Proof {
    pub(super) root: SymbolId,
    pub(super) initializer: ExpressionId,
    pub(super) consumer: ExpressionId,
    pub(super) birth: CleanupOwnerValueId,
    pub(super) header: CleanupOwnerValueId,
    pub(super) exit: CleanupOwnerValueId,
    /// The actual final consumption, not necessarily this provider's exhaustion.
    pub(super) drop: DropFact,
    pub(super) late_size: Option<ExpressionId>,
    pub(super) dead: Vec<DeadColumn>,
    pub(super) path: Option<Path>,
}

#[derive(Clone, Copy)]
pub(super) struct DeadColumn {
    pub(super) root: SymbolId,
    pub(super) initializer: ExpressionId,
    pub(super) previous: StatementId,
    pub(super) consumed: DropFact,
    pub(super) drop: DropFact,
    pub(super) header: CleanupOwnerValueId,
    pub(super) exit: CleanupOwnerValueId,
}

/// A precise FE control identity, never permission to assume a branch executed.
/// The lowering carrier must prove this successful arm dominates the preheader.
#[derive(Clone, Copy)]
pub(super) struct Path {
    pub(super) control: ExpressionId,
    pub(super) condition: ExpressionId,
    pub(super) selector: CleanupSelectorId,
    pub(super) branch: usize,
    pub(super) success: StatementId,
    pub(super) failure: StatementId,
    pub(super) size: ExpressionId,
    pub(super) previous: StatementId,
}

pub(super) fn qualify<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    plan: &'a MapIterationOwnershipPlan,
    body: StatementId,
    span: Span,
) -> Result<qualify::Qualified<'a>, LoweringError> {
    qualify_scope(parsed, names, typed, owned, plan, body, span).map_err(|failure| {
        if failure.kind == LoweringErrorKind::UnsupportedNode {
            error(LoweringErrorKind::UnsupportedNode, span)
        } else {
            failure
        }
    })
}

fn qualify_scope<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    plan: &'a MapIterationOwnershipPlan,
    body: StatementId,
    span: Span,
) -> Result<qualify::Qualified<'a>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let sites = identity::collect(parsed, names, typed, owned, plan, span)?;
    let mut proofs = Vec::new();
    for (index, site) in sites.iter().enumerate() {
        let previous = index
            .checked_sub(1)
            .map(|index| (&sites[index], &proofs[index]));
        let proof = topology::prove(parsed, owned, site, previous)?;
        proofs.push(proof);
    }
    // Match the complete root-or-associated-owner Drop set, including later dead
    // columns. No unrelated target can hide behind a known owner identity.
    let mut expected = Vec::new();
    let mut owners = Vec::new();
    for proof in &proofs {
        expected.push(proof.drop);
        owners.extend([proof.birth, proof.header, proof.exit]);
        for dead in &proof.dead {
            expected.push(dead.drop);
            owners.extend([dead.header, dead.exit]);
        }
    }
    if expected
        .iter()
        .enumerate()
        .any(|(index, fact)| expected[..index].contains(fact))
    {
        return Err(missing());
    }
    let related = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| {
            sites
                .iter()
                .any(|site| fact.target() == DropTarget::Named(site.root))
                || fact.owner().is_some_and(|owner| owners.contains(&owner))
        })
        .collect::<Vec<_>>();
    if related.len() != expected.len()
        || expected
            .iter()
            .any(|fact| related.iter().filter(|actual| *actual == fact).count() != 1)
    {
        return Err(missing());
    }
    let index = sites
        .iter()
        .position(|site| site.plan.descriptor().statement() == plan.descriptor().statement())
        .ok_or_else(missing)?;
    let site = &sites[index];
    if site.body != body {
        return Err(missing());
    }
    let proof = proofs.remove(index);
    Ok(qualify::Qualified {
        plan,
        source: SourceIdentity::Local {
            symbol: proof.root,
            initializer: proof.initializer,
        },
        body,
        origin: site.origin,
        owner: Some(source_owner::Proof {
            flow: None,
            disposition: source_owner::Disposition::Drop(proof.drop),
            transfer: None,
            conditional_return: None,
        }),
        body_exit: IterationExitKind::Fallthrough,
        conditional: None,
        nesting: nested::Nesting { parent: None },
        local: Some(proof),
    })
}

/// Called only after the fallible whole-descriptor qualification preflight.
/// A false result is not used to translate a qualification corruption into U.
pub(super) fn supported_drop(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> bool {
    typed.map_iterations().iter().any(|descriptor| {
        let Ok(qualified) = qualify::qualify(parsed, names, typed, owned, *descriptor) else {
            return false;
        };
        qualified.local.is_some_and(|proof| {
            proof.drop == fact || proof.dead.iter().any(|dead| dead.drop == fact)
        })
    })
}

pub(super) mod consumer_prove {
    use super::*;

    pub(super) fn prove(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        descriptor: &MapIterationDescriptor,
        body: StatementId,
        span: Span,
    ) -> Result<ExpressionId, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let body = parsed.ast().statements().get(body).map_err(|_| missing())?;
        let Statement::Block { elements } = body.payload() else {
            return Err(unsupported());
        };
        let [statement] = elements.as_slice() else {
            return Err(unsupported());
        };
        let statement = parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|_| missing())?;
        let Statement::Expression { expression } = statement.payload() else {
            return Err(unsupported());
        };
        let node = parsed
            .ast()
            .expressions()
            .get(*expression)
            .map_err(|_| missing())?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = node.payload()
        else {
            return Err(unsupported());
        };
        if arguments.len() != 2
            || !type_arguments.is_empty()
            || arguments
                .iter()
                .any(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some())
        {
            return Err(unsupported());
        }
        let callee = parsed
            .ast()
            .expressions()
            .get(*callee)
            .map_err(|_| missing())?;
        if callee.payload() != &Expression::Name {
            return Err(unsupported());
        }
        let call = typed.call(*expression).ok_or_else(missing)?;
        let CallableTarget::Source(target) = call.target() else {
            return Err(unsupported());
        };
        let symbol = names.symbols().get(target.index()).ok_or_else(missing)?;
        let signature = unique(
            typed
                .callables()
                .iter()
                .filter(|signature| signature.symbol() == target),
        )
        .ok_or_else(missing)?;
        if symbol.kind() != SymbolKind::Function
            || !parsed.roots().iter().any(|item| parsed.ast().items().get(*item).is_ok_and(|node| {
                matches!(node.payload(), Item::Function { name: NameMarker::Present(marker), .. } if *marker == symbol.span())
            }))
            || call.receiver().is_some() || call.aborts() || call.prints_line()
            || !call.instance().type_arguments().is_empty() || signature.owner().is_some()
            || signature.receiver().is_some() || !signature.type_parameters().is_empty()
            || builtin_type(typed, call.return_type()) != Some(BuiltinType::Unit)
        {
            return Err(unsupported());
        }
        let reference = unique(names.references().iter().filter(|reference| {
            reference.span() == callee.span() && reference.namespace() == Namespace::Value
        }))
        .ok_or_else(missing)?;
        let selected = match reference.target() {
            ReferenceTarget::Symbol(symbol) => *symbol == target,
            ReferenceTarget::OverloadSet(symbols) => symbols.contains(&target),
            _ => false,
        };
        if !selected
            || call.expression() != *expression
            || call.result_source() != CallableResultSource::Owned
            || signature.result_source() != call.result_source()
            || signature.return_type() != call.return_type()
            || typed.expression_type(*expression) != Some(call.return_type())
            || call.arguments().len() != 2
            || signature.parameters().len() != 2
        {
            return Err(missing());
        }
        let loans = owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == *expression)
            .collect::<Vec<_>>();
        let ends = owned
            .loan_ends()
            .iter()
            .filter(|end| end.call() == *expression)
            .collect::<Vec<_>>();
        if loans.len() != 2 || ends.len() != 2 {
            return Err(missing());
        }
        for (index, argument) in arguments.iter().enumerate() {
            let symbol = descriptor.bindings()[index];
            let ty = [descriptor.key_type(), descriptor.value_type()][index];
            let argument_node = parsed
                .ast()
                .expressions()
                .get(argument.value)
                .map_err(|_| missing())?;
            if argument_node.payload() != &Expression::Name {
                return Err(unsupported());
            }
            let reference = unique(names.references().iter().filter(|reference| {
                reference.span() == argument_node.span()
                    && reference.namespace() == Namespace::Value
            }))
            .ok_or_else(missing)?;
            let parameter = &signature.parameters()[index];
            let mapping = call.arguments()[index];
            let loan = unique(
                loans
                    .iter()
                    .copied()
                    .filter(|loan| loan.argument() == argument.value),
            )
            .ok_or_else(missing)?;
            let end = unique(
                ends.iter()
                    .copied()
                    .filter(|end| end.argument() == argument.value),
            )
            .ok_or_else(missing)?;
            if reference.target() != &ReferenceTarget::Symbol(symbol)
                || parameter.mode != ParameterMode::Borrow
                || parameter.ty != ty
                || mapping.argument_index() != index
                || mapping.parameter_index() != index
                || mapping.mode() != ParameterMode::Borrow
                || mapping.parameter_type() != ty
                || mapping.crosses_thread()
                || mapping.category() != ExpressionCategory::Place
                || typed.expression_type(argument.value) != Some(ty)
                || typed.expression_category(argument.value) != Some(ExpressionCategory::Place)
                || loan.kind() != LoanKind::Shared
                || loan.begin_span() != argument_node.span()
                || loan.end_span() != node.span()
                || loan.is_receiver_reservation()
                || loan.activation_point().is_some()
                || !matches!(loan.target(), LoanTarget::Place(place) if place.is_root() && place.root() == symbol)
                || end.point() != LoanEndPoint::CallReturn(*expression)
            {
                return Err(missing());
            }
        }
        Ok(*expression)
    }
}
