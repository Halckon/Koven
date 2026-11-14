//! Consume the sealed core no-carry fact; never infer fresh-loop legality from syntax alone.
use lang_frontend::{
    ast::StatementId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, CompilationUnitOwnership, UnitLoanTarget,
    },
    parser::{ParsedFile, Statement},
    type_checking::UnitExpressionId,
};

pub(in crate::ssa) fn admitted(
    file: &ParsedFile,
    owned: &CompilationUnitOwnership,
    binding: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
) -> Option<StatementId> {
    let mut selected = owned
        .no_carry_loops()
        .iter()
        .filter(|proof| proof.binding() == binding.binding());
    let proof = selected.next()?;
    let source = binding.source_loan()?;
    if selected.next().is_some()
        || !owned.owns_no_carry_loop(proof)
        || binding.storage() != BorrowBindingStorage::BorrowedStorage
        || binding.parent().is_some()
        || proof.initializer() != binding.initializer()
        || proof.initializer() != source.call()
        || proof.source_argument() != source.argument()
        || proof.body().source_unit() != binding.binding().source_unit()
        || proof.loop_statement().source_unit() != binding.binding().source_unit()
        || proof.callable().source_unit() != binding.binding().source_unit()
        || file.source_id() != binding.marker_span().source_id()
        || !matches!(binding.origin(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == proof.source())
    {
        return None;
    }
    let mut ends = owned
        .borrow_results()
        .ends()
        .iter()
        .filter(|e| e.binding() == binding.binding());
    if ends.next()?.point() != proof.end() || ends.next().is_some() {
        return None;
    }
    let Statement::While { body, .. } = file
        .ast()
        .statements()
        .get(proof.loop_statement().statement())
        .ok()?
        .payload()
    else {
        return None;
    };
    (*body == proof.body().statement()).then_some(proof.loop_statement().statement())
}

/// Direct peers grant permission only to a binding with its own sealed no-carry proof.
pub(in crate::ssa) fn siblings(
    file: &ParsedFile,
    owned: &CompilationUnitOwnership,
    binding: &BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
) -> Vec<StatementId> {
    let Some(current_statement) = admitted(file, owned, binding) else {
        return Vec::new();
    };
    let Some(current) = owned.no_carry_loops().iter().find(|proof| {
        proof.binding() == binding.binding()
            && proof.loop_statement().statement() == current_statement
    }) else {
        return Vec::new();
    };
    let Some(elements) = direct_elements(file, current.callable().item()) else {
        return Vec::new();
    };
    if !elements.contains(&current_statement) {
        return Vec::new();
    }
    owned
        .no_carry_loops()
        .iter()
        .filter_map(|peer| {
            if !owned.owns_no_carry_loop(peer)
                || peer.callable() != current.callable()
                || peer.source() != current.source()
                || peer.binding() == current.binding()
                || peer.body() == current.body()
                || peer.loop_statement() == current.loop_statement()
                || peer.loop_statement().source_unit() != current.loop_statement().source_unit()
                || !elements.contains(&peer.loop_statement().statement())
            {
                return None;
            }
            let mut bindings = owned
                .borrow_results()
                .bindings()
                .iter()
                .filter(|b| b.binding() == peer.binding());
            let actual = bindings.next()?;
            if bindings.next().is_some()
                || admitted(file, owned, actual) != Some(peer.loop_statement().statement())
            {
                return None;
            }
            Some(peer.loop_statement().statement())
        })
        .collect()
}

fn direct_elements(
    file: &ParsedFile,
    callable: lang_frontend::ast::ItemId,
) -> Option<&[StatementId]> {
    use lang_frontend::parser::{FunctionBody, FunctionForm, Item};
    let Item::Function { form, .. } = file.ast().items().get(callable).ok()?.payload() else {
        return None;
    };
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body) => *body,
        FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return None,
    };
    let Statement::Block { elements } = file.ast().statements().get(body).ok()?.payload() else {
        return None;
    };
    Some(elements)
}
