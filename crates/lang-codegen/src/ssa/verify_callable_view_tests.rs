//! ADR-0034 callable views at the SSA layer (SPEC-0364 step 1): the positive shapes verify
//! and render, and every escape or lifetime violation is rejected by the verifier itself.
use lang_frontend::source::SourceMap;

use super::{
    model::{
        CallableSignature, ClosureCaptureMode, ClosureCaptureOperand, ClosureCaptureType, EntityId,
        EntityType, FunctionId, LoanId, LoanKind, Module, Operation, Origin, Program, SsaTypeId,
        SsaTypeKind, TerminatorKind, ValueId,
    },
    verify::verify_program,
};

/// The verifier's errors for `program`, rendered for reason matching.
fn rejection(program: &Program) -> String {
    format!(
        "{:?}",
        verify_program(program)
            .expect_err("the defect must be rejected")
            .errors
            .into_iter()
            .map(|error| error.kind)
            .collect::<Vec<_>>()
    )
}

const INT: SsaTypeKind = SsaTypeKind::Integer {
    bits: 32,
    signed: true,
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("callable-view.ko", "fun apply(f: (Int) -> Int)")
        .expect("unique test source");
    Origin::Source(sources.span(source, 0, 3).expect("valid span"))
}

struct Types {
    int: SsaTypeId,
    pointer: SsaTypeId,
    view: SsaTypeId,
}

fn types(module: &mut Module) -> Types {
    let int = module.intern_type(INT);
    let pointer = module
        .add_function_pointer_type(vec![int], vec![int])
        .expect("pointer type");
    let view = module
        .add_callable_view_type(CallableSignature {
            parameters: vec![EntityType::Value(int)],
            returns: vec![int],
        })
        .expect("view type");
    Types { int, pointer, view }
}

fn view_loan(view: SsaTypeId) -> EntityType {
    EntityType::Loan {
        kind: LoanKind::Shared,
        target: view,
    }
}

fn values(results: &[EntityId]) -> Vec<ValueId> {
    results
        .iter()
        .map(|entity| match entity {
            EntityId::Value(value) => *value,
            _ => panic!("value result"),
        })
        .collect()
}

fn loan(results: &[EntityId]) -> LoanId {
    match results {
        [EntityId::Loan(loan)] => *loan,
        _ => panic!("one loan result"),
    }
}

/// `fun inc(n: Int): Int = n`.
fn inc(module: &mut Module, types: &Types) -> FunctionId {
    let origin = origin();
    let id = module
        .add_function("inc", vec![types.int], origin.clone())
        .expect("inc");
    let function = module.function_mut(id).expect("inc");
    let entry = function
        .add_block(vec![EntityType::Value(types.int)], origin.clone())
        .expect("entry");
    let parameter = function.block(entry).expect("entry").parameters[0];
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: values(&[parameter]),
            },
            origin,
        )
        .expect("return");
    id
}

/// `fun apply(f: <view>, x: Int): Int = f(x)` with `f` typed `parameter`.
fn apply(module: &mut Module, types: &Types, parameter: EntityType) -> FunctionId {
    let origin = origin();
    let id = module
        .add_function("apply", vec![types.int], origin.clone())
        .expect("apply");
    let function = module.function_mut(id).expect("apply");
    let entry = function
        .add_block(
            vec![parameter, EntityType::Value(types.int)],
            origin.clone(),
        )
        .expect("entry");
    let parameters = function.block(entry).expect("entry").parameters.clone();
    let EntityId::Loan(view) = parameters[0] else {
        // A non-loan view parameter is itself the defect under test; return the argument.
        function
            .set_terminator(
                entry,
                TerminatorKind::Return {
                    values: values(&parameters[1..]),
                },
                origin,
            )
            .expect("return");
        return id;
    };
    let (_, results) = function
        .append_instruction(
            entry,
            Operation::CallableInvokeView {
                view,
                arguments: vec![parameters[1]],
            },
            vec![EntityType::Value(types.int)],
            origin.clone(),
        )
        .expect("invoke view");
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: values(&results),
            },
            origin,
        )
        .expect("return");
    id
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Defect {
    None,
    /// The source loan ends while its view is still active.
    SourceEndsFirst,
    /// The view is used after its source loan ended.
    UseAfterSourceEnd,
    /// A view is taken of a loan that is not callable.
    NotCallable,
}

/// `fun main(x: Int): Int { val f = ::inc; return apply(view(&f), x) }`.
fn caller_program(defect: Defect) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module = program.add_module("main");
    let module = program.module_mut(module).expect("module");
    let types = types(module);
    let inc = inc(module, &types);
    let apply = apply(module, &types, view_loan(types.view));
    let main = module
        .add_function("main", vec![types.int], origin.clone())
        .expect("main");
    let function = module.function_mut(main).expect("main");
    let entry = function
        .add_block(vec![EntityType::Value(types.int)], origin.clone())
        .expect("entry");
    let x = function.block(entry).expect("entry").parameters[0];
    let EntityId::Value(x_value) = x else {
        panic!("value parameter")
    };
    let owner = if defect == Defect::NotCallable {
        x_value
    } else {
        let (_, results) = function
            .append_instruction(
                entry,
                Operation::FunctionAddress { target: inc },
                vec![EntityType::Value(types.pointer)],
                origin.clone(),
            )
            .expect("address");
        values(&results)[0]
    };
    let owner_type = if defect == Defect::NotCallable {
        types.int
    } else {
        types.pointer
    };
    let (_, place) = function
        .append_instruction(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(owner_type)],
            origin.clone(),
        )
        .expect("place");
    let EntityId::Place(place) = place[0] else {
        panic!("place")
    };
    let (_, source) = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner_type,
            }],
            origin.clone(),
        )
        .expect("borrow");
    let source = loan(&source);
    let (_, view) = function
        .append_instruction(
            entry,
            Operation::CallableViewOf { source },
            vec![view_loan(types.view)],
            origin.clone(),
        )
        .expect("view");
    let view = loan(&view);
    if defect == Defect::UseAfterSourceEnd {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: source },
                vec![],
                origin.clone(),
            )
            .expect("end source");
    }
    let (_, result) = function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee: apply,
                receiver: None,
                arguments: vec![EntityId::Loan(view), x],
            },
            vec![EntityType::Value(types.int)],
            origin.clone(),
        )
        .expect("call");
    let ends = match defect {
        Defect::SourceEndsFirst => vec![source, view],
        Defect::UseAfterSourceEnd => vec![view],
        Defect::None | Defect::NotCallable => vec![view, source],
    };
    for loan in ends {
        function
            .append_instruction(entry, Operation::BorrowEnd { loan }, vec![], origin.clone())
            .expect("end");
    }
    if defect != Defect::NotCallable {
        function
            .append_instruction(entry, Operation::Drop { owner }, vec![], origin.clone())
            .expect("drop pointer");
    }
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: values(&result),
            },
            origin,
        )
        .expect("return");
    program
}

#[test]
fn function_pointer_view_verifies_and_calls_through_a_trampoline() {
    let program = caller_program(Defect::None);
    verify_program(&program).expect("a borrowed function pointer view is valid");
    let llvm = crate::llvm::render_verified_program(&program).expect("view lowers to LLVM");
    assert!(llvm.contains("koven.callable_view.trampoline"), "{llvm}");
    assert!(!llvm.contains("@malloc"), "views never allocate: {llvm}");
}

#[test]
fn view_lifetime_violations_are_rejected() {
    for defect in [Defect::SourceEndsFirst, Defect::UseAfterSourceEnd] {
        let reason = rejection(&caller_program(defect));
        assert!(reason.contains("LoanDependencyActive"), "{reason}");
    }
    let reason = rejection(&caller_program(Defect::NotCallable));
    assert!(reason.contains("OperationContract"), "{reason}");
}

fn module_with_types() -> (Program, Types) {
    let mut program = Program::default();
    let module = program.add_module("main");
    let types = types(program.module_mut(module).expect("module"));
    (program, types)
}

#[test]
fn a_view_is_never_a_value_parameter() {
    let (mut program, types) = module_with_types();
    let module = program.modules.first_mut().expect("module");
    apply(module, &types, EntityType::Value(types.view));
    let reason = rejection(&program);
    assert!(reason.contains("only a shared loan target"), "{reason}");
}

#[test]
fn a_view_is_never_returned() {
    let (mut program, types) = module_with_types();
    let origin = origin();
    let module = program.modules.first_mut().expect("module");
    let id = module
        .add_function("leak", vec![types.view], origin.clone())
        .expect("signature");
    let function = module.function_mut(id).expect("leak");
    let entry = function
        .add_block(vec![view_loan(types.view)], origin.clone())
        .expect("entry");
    function
        .set_terminator(entry, TerminatorKind::Abort, origin)
        .expect("terminator");
    let reason = rejection(&program);
    assert!(reason.contains("cannot be returned"), "{reason}");
}

#[test]
fn a_view_is_never_stored_in_owned_storage() {
    for store in 0..3 {
        let (mut program, types) = module_with_types();
        let module = program.modules.first_mut().expect("module");
        match store {
            0 => {
                module
                    .add_aggregate_type("Holder", vec![types.view])
                    .expect("type table accepts the shape; the verifier rejects it");
            }
            1 => {
                module.intern_type(SsaTypeKind::InlineNullable {
                    inner: types.view,
                    niche: false,
                });
            }
            _ => {
                let environment = module
                    .add_aggregate_type("OwnedViewEnv", vec![types.view])
                    .expect("environment");
                module
                    .add_concrete_closure_type(
                        "OwnedView",
                        vec![],
                        vec![],
                        environment,
                        vec![ClosureCaptureType {
                            mode: ClosureCaptureMode::Owned,
                            ty: types.view,
                        }],
                    )
                    .expect("closure type");
            }
        }
        let reason = rejection(&program);
        assert!(
            reason.contains("cannot be stored"),
            "owned view storage {store}: {reason}"
        );
    }
}

/// `fun capture(f: <view>): <closure> = { f(1) }` — a borrowed closure that share-captures a
/// view must not be returned (`closure_escape`); used locally it verifies.
fn capturing_program(escape: bool) -> Program {
    let origin = origin();
    let (mut program, types) = module_with_types();
    let module = program.modules.first_mut().expect("module");
    let reference = module
        .add_shared_reference_type(types.view)
        .expect("reference");
    let environment = module
        .add_aggregate_type("ViewEnv", vec![reference])
        .expect("environment");
    let closure = module
        .add_concrete_closure_type(
            "ViewClosure",
            vec![],
            vec![],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: types.view,
            }],
        )
        .expect("closure");
    let thunk = module
        .add_function("thunk", vec![], origin.clone())
        .expect("thunk");
    let body = module.function_mut(thunk).expect("thunk");
    let thunk_entry = body
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: environment,
            }],
            origin.clone(),
        )
        .expect("entry");
    body.set_terminator(
        thunk_entry,
        TerminatorKind::Return { values: vec![] },
        origin.clone(),
    )
    .expect("return");
    let returns = if escape { vec![closure] } else { vec![] };
    let id = module
        .add_function("capture", returns, origin.clone())
        .expect("capture");
    let function = module.function_mut(id).expect("capture");
    let entry = function
        .add_block(vec![view_loan(types.view)], origin.clone())
        .expect("entry");
    let EntityId::Loan(view) = function.block(entry).expect("entry").parameters[0] else {
        panic!("view parameter")
    };
    let (_, built) = function
        .append_instruction(
            entry,
            Operation::ClosureConstruct {
                closure,
                thunk,
                captures: vec![ClosureCaptureOperand::Shared(view)],
            },
            vec![EntityType::Value(closure)],
            origin.clone(),
        )
        .expect("construct");
    let value = values(&built)[0];
    let returned = if escape {
        vec![value]
    } else {
        function
            .append_instruction(
                entry,
                Operation::CallableInvoke {
                    callable: value,
                    arguments: vec![],
                },
                vec![],
                origin.clone(),
            )
            .expect("invoke");
        function
            .append_instruction(
                entry,
                Operation::Drop { owner: value },
                vec![],
                origin.clone(),
            )
            .expect("drop");
        vec![]
    };
    function
        .set_terminator(entry, TerminatorKind::Return { values: returned }, origin)
        .expect("return");
    program
}

#[test]
fn a_closure_capturing_a_view_stays_borrowed() {
    verify_program(&capturing_program(false)).expect("local borrowed use of a view capture");
    let reason = rejection(&capturing_program(true));
    assert!(
        reason.contains("borrowed closure cannot escape"),
        "a closure capturing a view cannot escape its function: {reason}"
    );
}

/// `fun main(x: Int): Int { val k = x; val c = { n -> n }  /* captures k by value */;
/// return apply(view(&c), x) }` — a concrete closure contributes its thunk and environment.
fn closure_caller_program() -> Program {
    let origin = origin();
    let (mut program, types) = module_with_types();
    let module = program.modules.first_mut().expect("module");
    let apply = apply(module, &types, view_loan(types.view));
    let environment = module
        .add_aggregate_type("IntEnv", vec![types.int])
        .expect("environment");
    let closure = module
        .add_concrete_closure_type(
            "IntClosure",
            vec![types.int],
            vec![types.int],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: types.int,
            }],
        )
        .expect("closure");
    let thunk = module
        .add_function("thunk", vec![types.int], origin.clone())
        .expect("thunk");
    let body = module.function_mut(thunk).expect("thunk");
    let thunk_entry = body
        .add_block(
            vec![
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: environment,
                },
                EntityType::Value(types.int),
            ],
            origin.clone(),
        )
        .expect("entry");
    let n = body.block(thunk_entry).expect("entry").parameters[1];
    body.set_terminator(
        thunk_entry,
        TerminatorKind::Return {
            values: values(&[n]),
        },
        origin.clone(),
    )
    .expect("return");
    let main = module
        .add_function("main", vec![types.int], origin.clone())
        .expect("main");
    let function = module.function_mut(main).expect("main");
    let entry = function
        .add_block(vec![EntityType::Value(types.int)], origin.clone())
        .expect("entry");
    let x = function.block(entry).expect("entry").parameters[0];
    let (_, copied) = function
        .append_instruction(
            entry,
            Operation::Copy {
                source: values(&[x])[0],
            },
            vec![EntityType::Value(types.int)],
            origin.clone(),
        )
        .expect("copy");
    let (_, built) = function
        .append_instruction(
            entry,
            Operation::ClosureConstruct {
                closure,
                thunk,
                captures: vec![ClosureCaptureOperand::Owned(values(&copied)[0])],
            },
            vec![EntityType::Value(closure)],
            origin.clone(),
        )
        .expect("construct");
    let owner = values(&built)[0];
    let (_, place) = function
        .append_instruction(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(closure)],
            origin.clone(),
        )
        .expect("place");
    let EntityId::Place(place) = place[0] else {
        panic!("place")
    };
    let (_, source) = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: closure,
            }],
            origin.clone(),
        )
        .expect("borrow");
    let source = loan(&source);
    let (_, view) = function
        .append_instruction(
            entry,
            Operation::CallableViewOf { source },
            vec![view_loan(types.view)],
            origin.clone(),
        )
        .expect("view");
    let view = loan(&view);
    let (_, result) = function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee: apply,
                receiver: None,
                arguments: vec![EntityId::Loan(view), x],
            },
            vec![EntityType::Value(types.int)],
            origin.clone(),
        )
        .expect("call");
    for loan in [view, source] {
        function
            .append_instruction(entry, Operation::BorrowEnd { loan }, vec![], origin.clone())
            .expect("end");
    }
    function
        .append_instruction(entry, Operation::Drop { owner }, vec![], origin.clone())
        .expect("drop closure");
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: values(&result),
            },
            origin,
        )
        .expect("return");
    program
}

#[test]
fn closure_view_passes_thunk_and_borrowed_environment() {
    let program = closure_caller_program();
    verify_program(&program).expect("a borrowed concrete closure view is valid");
    let llvm = crate::llvm::render_verified_program(&program).expect("view lowers to LLVM");
    assert!(!llvm.contains("koven.callable_view.trampoline"), "{llvm}");
    assert!(!llvm.contains("@malloc"), "views never allocate: {llvm}");
}
