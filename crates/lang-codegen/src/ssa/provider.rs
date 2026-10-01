//! 借用式顺序 provider 的固定 SSA 算法；owner/loan 的边运输由调用方显式提供。

use super::model::{
    BinaryOperator, BlockId, ComparisonOperator, Edge, EntityId, EntityType, Function, LoanId,
    LoanKind, ModelError, Operation, Origin, PlaceId, ScalarConstant, SsaTypeId, TerminatorKind,
    ValueId,
};

pub(crate) struct ProviderSnapshot {
    source: LoanId,
    length: ValueId,
    cursor: ValueId,
    step: ValueId,
}

impl ProviderSnapshot {
    pub(crate) const fn length(&self) -> ValueId {
        self.length
    }

    pub(crate) const fn cursor(&self) -> ValueId {
        self.cursor
    }
}

pub(crate) struct ProviderHeader {
    pub(crate) block: BlockId,
    pub(crate) source: LoanId,
    pub(crate) length: ValueId,
    pub(crate) cursor: ValueId,
    pub(crate) source_slot: usize,
    pub(crate) length_slot: usize,
    pub(crate) cursor_slot: usize,
    pub(crate) step: ValueId,
}

pub(crate) struct ProviderNext {
    pub(crate) header: BlockId,
    pub(crate) source: LoanId,
    pub(crate) value: ValueId,
}

impl ProviderNext {
    #[cfg(test)]
    pub(crate) const fn value(&self) -> ValueId {
        self.value
    }
}

impl ProviderHeader {
    pub(crate) fn backedge(
        &self,
        mut arguments: Vec<EntityId>,
        next: ProviderNext,
    ) -> Result<Edge, ProviderBuildError> {
        if next.header != self.block
            || arguments.len() <= self.source_slot
            || arguments.len() <= self.length_slot
            || arguments.len() <= self.cursor_slot
        {
            return Err(ProviderBuildError::InvalidHeaderTransport { block: self.block });
        }
        arguments[self.source_slot] = EntityId::Loan(next.source);
        arguments[self.length_slot] = EntityId::Value(self.length);
        arguments[self.cursor_slot] = EntityId::Value(next.value);
        Ok(Edge {
            target: self.block,
            arguments,
        })
    }

    #[cfg(test)]
    pub(crate) const fn step(&self) -> ValueId {
        self.step
    }
}

pub(crate) struct GuardedElement {
    loan: LoanId,
    header: BlockId,
    body: BlockId,
    source: LoanId,
    cursor: ValueId,
    step: ValueId,
}

#[derive(Debug)]
pub(crate) enum ProviderBuildError {
    Model(ModelError),
    MissingBodyTransport { block: BlockId },
    InvalidHeaderTransport { block: BlockId },
}

impl From<ModelError> for ProviderBuildError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

pub(crate) fn snapshot(
    function: &mut Function,
    preheader: BlockId,
    source: LoanId,
    int_type: SsaTypeId,
    origin: &Origin,
) -> Result<ProviderSnapshot, ModelError> {
    let length = append_value(
        function,
        preheader,
        Operation::ContainerLength {
            owner: EntityId::Loan(source),
        },
        int_type,
        origin,
    )?;
    let cursor = append_value(
        function,
        preheader,
        Operation::Constant(ScalarConstant::Integer(0)),
        int_type,
        origin,
    )?;
    let step = append_value(
        function,
        preheader,
        Operation::Constant(ScalarConstant::Integer(1)),
        int_type,
        origin,
    )?;
    Ok(ProviderSnapshot {
        source,
        length,
        cursor,
        step,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn enter_header(
    function: &mut Function,
    preheader: BlockId,
    header: BlockId,
    snapshot: ProviderSnapshot,
    mut arguments: Vec<EntityId>,
    source_slot: usize,
    length_slot: usize,
    cursor_slot: usize,
    origin: &Origin,
) -> Result<ProviderHeader, ProviderBuildError> {
    let parameters = &function
        .block(header)
        .ok_or(ModelError::UnknownBlock { block: header })?
        .parameters;
    if source_slot == length_slot
        || source_slot == cursor_slot
        || length_slot == cursor_slot
        || arguments.len() != parameters.len()
        || !matches!(parameters.get(source_slot), Some(EntityId::Loan(_)))
        || !matches!(parameters.get(length_slot), Some(EntityId::Value(_)))
        || !matches!(parameters.get(cursor_slot), Some(EntityId::Value(_)))
    {
        return Err(ProviderBuildError::InvalidHeaderTransport { block: header });
    }
    let (EntityId::Loan(source), EntityId::Value(length), EntityId::Value(cursor)) = (
        parameters[source_slot],
        parameters[length_slot],
        parameters[cursor_slot],
    ) else {
        unreachable!("header value slots were checked")
    };
    arguments[source_slot] = EntityId::Loan(snapshot.source);
    arguments[length_slot] = EntityId::Value(snapshot.length);
    arguments[cursor_slot] = EntityId::Value(snapshot.cursor);
    function.set_terminator(
        preheader,
        TerminatorKind::Branch(Edge {
            target: header,
            arguments,
        }),
        origin.clone(),
    )?;
    Ok(ProviderHeader {
        block: header,
        source,
        length,
        cursor,
        source_slot,
        length_slot,
        cursor_slot,
        step: snapshot.step,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn guard_and_begin(
    function: &mut Function,
    header: &ProviderHeader,
    bool_type: SsaTypeId,
    element_type: SsaTypeId,
    body: Edge,
    exit: Edge,
    origin: &Origin,
) -> Result<GuardedElement, ProviderBuildError> {
    let body_id = body.target;
    let body_block = function
        .block(body_id)
        .ok_or(ModelError::UnknownBlock { block: body_id })?;
    let source_index = body
        .arguments
        .iter()
        .position(|argument| *argument == EntityId::Loan(header.source))
        .ok_or(ProviderBuildError::MissingBodyTransport { block: body_id })?;
    let cursor_index = body
        .arguments
        .iter()
        .position(|argument| *argument == EntityId::Value(header.cursor))
        .ok_or(ProviderBuildError::MissingBodyTransport { block: body_id })?;
    let Some(EntityId::Loan(body_source)) = body_block.parameters.get(source_index) else {
        return Err(ProviderBuildError::MissingBodyTransport { block: body_id });
    };
    let Some(EntityId::Value(body_cursor)) = body_block.parameters.get(cursor_index) else {
        return Err(ProviderBuildError::MissingBodyTransport { block: body_id });
    };
    let (body_source, body_cursor) = (*body_source, *body_cursor);
    let has_next = append_value(
        function,
        header.block,
        Operation::Compare {
            operator: ComparisonOperator::LessThan,
            left: header.cursor,
            right: header.length,
        },
        bool_type,
        origin,
    )?;
    function.set_terminator(
        header.block,
        TerminatorKind::Conditional {
            condition: has_next,
            when_true: body,
            when_false: exit,
        },
        origin.clone(),
    )?;
    let place = append_place(
        function,
        body_id,
        Operation::ContainerElementPlace {
            owner: EntityId::Loan(body_source),
            index: body_cursor,
        },
        element_type,
        origin,
    )?;
    let (_, results) = function.append_instruction(
        body_id,
        Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        },
        vec![EntityType::Loan {
            kind: LoanKind::Shared,
            target: element_type,
        }],
        origin.clone(),
    )?;
    let EntityId::Loan(loan) = results[0] else {
        unreachable!("BorrowBegin result type was supplied as Loan")
    };
    Ok(GuardedElement {
        loan,
        header: header.block,
        body: body_id,
        source: body_source,
        cursor: body_cursor,
        step: header.step,
    })
}

impl GuardedElement {
    pub(crate) const fn loan(&self) -> LoanId {
        self.loan
    }

    pub(crate) const fn header(&self) -> BlockId {
        self.header
    }

    pub(crate) const fn body(&self) -> BlockId {
        self.body
    }

    pub(crate) const fn source(&self) -> LoanId {
        self.source
    }

    pub(crate) const fn cursor(&self) -> ValueId {
        self.cursor
    }

    pub(crate) const fn step(&self) -> ValueId {
        self.step
    }

    pub(crate) fn set_loan(&mut self, loan: LoanId) {
        self.loan = loan;
    }
}

pub(crate) fn finish_element_and_advance_in_block(
    function: &mut Function,
    block: BlockId,
    element: &GuardedElement,
    int_type: SsaTypeId,
    origin: &Origin,
) -> Result<ProviderNext, ModelError> {
    function.append_instruction(
        block,
        Operation::BorrowEnd { loan: element.loan },
        Vec::new(),
        origin.clone(),
    )?;
    let value = append_value(
        function,
        block,
        Operation::Binary {
            operator: BinaryOperator::Add,
            left: element.cursor,
            right: element.step,
        },
        int_type,
        origin,
    )?;
    Ok(ProviderNext {
        header: element.header,
        source: element.source,
        value,
    })
}

pub(crate) fn finish_element_and_advance(
    function: &mut Function,
    element: GuardedElement,
    int_type: SsaTypeId,
    origin: &Origin,
) -> Result<ProviderNext, ModelError> {
    finish_element_and_advance_in_block(function, element.body, &element, int_type, origin)
}

fn append_value(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    ty: SsaTypeId,
    origin: &Origin,
) -> Result<ValueId, ModelError> {
    let (_, results) = function.append_instruction(
        block,
        operation,
        vec![EntityType::Value(ty)],
        origin.clone(),
    )?;
    let EntityId::Value(value) = results[0] else {
        unreachable!("value result type was supplied")
    };
    Ok(value)
}

fn append_place(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    ty: SsaTypeId,
    origin: &Origin,
) -> Result<PlaceId, ModelError> {
    let (_, results) = function.append_instruction(
        block,
        operation,
        vec![EntityType::Place(ty)],
        origin.clone(),
    )?;
    let EntityId::Place(place) = results[0] else {
        unreachable!("place result type was supplied")
    };
    Ok(place)
}
