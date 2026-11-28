/-!
# F6 Slice 23: `expression.rs` / `expression_kernel.rs` 表达式核心分派与恢复守卫机器证明模型

本文件对应 `crates/lang-frontend/src/parser/engine/expression_kernel.rs` 中的纯栈值判定内核：
1. `prefix_operator_for_kind`（`!`/`+`/`-` 前缀运算符）
2. `is_when_body_line_break_infix_stop_kind`（`when` 臂体跨行中缀停止集合 `is`/`!is`/`in`/`!in`）
3. `when_condition_dispatch`（`when` 条件六路分派）
4. `return_parses_value`（`return` 携带返回值守卫）
5. `group_non_poison_tail_action`（括号分组非毒化尾部四路分派）
6. `string_missing_closer_emits_diagnostic`（字符串/插值缺失闭合防重报守卫）

证据等级：`proved (model, complete finite domain)`（5 条有限枚举/布尔全空间定理，零 Mathlib，仅核心公理）。
-/

namespace F6Expression

inductive ExprToken where
  | symBang
  | symPlus
  | symMinus
  | kwIs
  | symBangIs
  | kwIn
  | symBangIn
  | other
  deriving DecidableEq, Repr

inductive PrefixOperator where
  | opNot
  | opPlus
  | opMinus
  deriving DecidableEq, Repr

inductive WhenConditionDispatch where
  | typeTest (negated : Bool)
  | contains (negated : Bool)
  | expression
  | expectedWhenEntry
  deriving DecidableEq, Repr

inductive GroupNonPoisonTailAction where
  | atRightParen
  | recoverDeclarationTail
  | tryConfirmedCloser
  | noSkip
  deriving DecidableEq, Repr

def isWhenBodyLineBreakInfixStop (tok : ExprToken) : Bool :=
  match tok with
  | .kwIs | .symBangIs | .kwIn | .symBangIn => true
  | _ => false

def prefixOperatorFor (tok : ExprToken) : Option PrefixOperator :=
  match tok with
  | .symBang => some .opNot
  | .symPlus => some .opPlus
  | .symMinus => some .opMinus
  | _ => none

def whenConditionDispatch (tok : ExprToken) (canStartExpr : Bool) : WhenConditionDispatch :=
  match tok with
  | .kwIs => .typeTest false
  | .symBangIs => .typeTest true
  | .kwIn => .contains false
  | .symBangIn => .contains true
  | _ => if canStartExpr then .expression else .expectedWhenEntry

def isWhenConditionPrefixDispatch (d : WhenConditionDispatch) : Bool :=
  match d with
  | .typeTest _ | .contains _ => true
  | .expression | .expectedWhenEntry => false

def returnParsesValue
    (canStartExpr hasLineBreak inStops isControlStart : Bool) : Bool :=
  canStartExpr && !hasLineBreak && (!inStops || isControlStart)

def groupNonPoisonTailAction
    (atRightParen declarationTail atComma inOuterStops : Bool) : GroupNonPoisonTailAction :=
  if atRightParen then
    .atRightParen
  else if declarationTail then
    .recoverDeclarationTail
  else if atComma || !inOuterStops then
    .tryConfirmedCloser
  else
    .noSkip

def stringMissingCloserEmitsDiagnostic
    (lexerOwnsOwnerRecovery atSourceEnd terminalErrorAtEof : Bool) : Bool :=
  !(lexerOwnsOwnerRecovery || (atSourceEnd && terminalErrorAtEof))

theorem prefixOperator_and_whenInfixStop_spec (tok : ExprToken) :
    ((prefixOperatorFor tok).isSome = true ↔
      (tok = .symBang ∨ tok = .symPlus ∨ tok = .symMinus)) ∧
    (isWhenBodyLineBreakInfixStop tok = true ↔
      (tok = .kwIs ∨ tok = .symBangIs ∨ tok = .kwIn ∨ tok = .symBangIn)) ∧
    ((prefixOperatorFor tok).isSome = true → isWhenBodyLineBreakInfixStop tok = false) := by
  cases tok <;> decide

theorem whenConditionDispatch_spec (tok : ExprToken) (canStartExpr : Bool) :
    (whenConditionDispatch tok canStartExpr = .typeTest false ↔ tok = .kwIs) ∧
    (whenConditionDispatch tok canStartExpr = .typeTest true ↔ tok = .symBangIs) ∧
    (whenConditionDispatch tok canStartExpr = .contains false ↔ tok = .kwIn) ∧
    (whenConditionDispatch tok canStartExpr = .contains true ↔ tok = .symBangIn) ∧
    (isWhenConditionPrefixDispatch (whenConditionDispatch tok canStartExpr) =
      isWhenBodyLineBreakInfixStop tok) ∧
    (isWhenBodyLineBreakInfixStop tok = false →
      (whenConditionDispatch tok canStartExpr = .expression ↔ canStartExpr = true) ∧
      (whenConditionDispatch tok canStartExpr = .expectedWhenEntry ↔ canStartExpr = false)) := by
  cases tok <;> cases canStartExpr <;> decide

theorem returnParsesValue_spec
    (canStartExpr hasLineBreak inStops isControlStart : Bool) :
    (returnParsesValue canStartExpr hasLineBreak inStops isControlStart = true ↔
      canStartExpr = true ∧ hasLineBreak = false ∧ (inStops = false ∨ isControlStart = true)) ∧
    (hasLineBreak = true → returnParsesValue canStartExpr hasLineBreak inStops isControlStart = false) := by
  cases canStartExpr <;> cases hasLineBreak <;> cases inStops <;> cases isControlStart <;> decide

theorem groupNonPoisonTailAction_spec
    (atRightParen declarationTail atComma inOuterStops : Bool) :
    (groupNonPoisonTailAction atRightParen declarationTail atComma inOuterStops = .atRightParen ↔
      atRightParen = true) ∧
    (groupNonPoisonTailAction atRightParen declarationTail atComma inOuterStops = .recoverDeclarationTail ↔
      atRightParen = false ∧ declarationTail = true) ∧
    (groupNonPoisonTailAction atRightParen declarationTail atComma inOuterStops = .tryConfirmedCloser ↔
      atRightParen = false ∧ declarationTail = false ∧ (atComma = true ∨ inOuterStops = false)) ∧
    (groupNonPoisonTailAction atRightParen declarationTail atComma inOuterStops = .noSkip ↔
      atRightParen = false ∧ declarationTail = false ∧ atComma = false ∧ inOuterStops = true) := by
  cases atRightParen <;> cases declarationTail <;> cases atComma <;> cases inOuterStops <;> decide

theorem stringMissingCloserEmitsDiagnostic_spec
    (lexerOwnsOwnerRecovery atSourceEnd terminalErrorAtEof : Bool) :
    stringMissingCloserEmitsDiagnostic lexerOwnsOwnerRecovery atSourceEnd terminalErrorAtEof = true ↔
      lexerOwnsOwnerRecovery = false ∧ ¬(atSourceEnd = true ∧ terminalErrorAtEof = true) := by
  cases lexerOwnsOwnerRecovery <;> cases atSourceEnd <;> cases terminalErrorAtEof <;> decide

end F6Expression
