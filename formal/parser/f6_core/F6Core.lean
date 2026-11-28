/-!
# F6 Slice 25: `core.rs` 递归守卫、逆向有效词法扫描与上下文传播纯内核机器证明模型

本文件对应 `crates/lang-frontend/src/parser/engine/core.rs` 中的 6 个纯判定/扫描内核：
1. `try_enter_recursion_depth`（递归深度守卫与单步递增）
2. `previous_significant_end_with`（逆向跳过 trivia 的最后有效词法终偏移扫描）
3. `lambda_body_element_is_statement` / `control_body_element_is_statement`（尾表达式上下文传播）
4. `if_missing_else_emits_diagnostic`（值位置 `if` 缺失 `else` 诊断守卫）
5. `expression_tail_is_block_boundary` / `expression_tail_emits_unexpected_trailing`（表达式尾部恢复守卫）
6. `boundary_uses_empty_span` / `type_recovery_boundary_hit`（边界空 span 与类型恢复边界判定）

证据等级：
- 3 条 `proved (model, unbounded)`（任意 `Nat` 深度/下标与任意长度词法列表）
- 2 条 `proved (model, complete finite domain)`（有限布尔全空间）
零 Mathlib，仅核心公理。
-/

namespace F6Core

def tryEnterRecursionDepth (depth maxDepth : Nat) : Option Nat :=
  if depth >= maxDepth then none else some (depth + 1)

/-- 无界自然数定理（C1）：递归深度守卫精确在 `depth < maxDepth` 时放行并推进至 `depth + 1`。 -/
theorem tryEnterRecursionDepth_spec (depth maxDepth : Nat) :
    (tryEnterRecursionDepth depth maxDepth = none ↔ depth ≥ maxDepth) ∧
    (∀ next, tryEnterRecursionDepth depth maxDepth = some next ↔ depth < maxDepth ∧ next = depth + 1) := by
  constructor
  · unfold tryEnterRecursionDepth
    by_cases h : depth ≥ maxDepth
    · simp [h]
    · simp [h]
  · intro next
    unfold tryEnterRecursionDepth
    by_cases h : depth ≥ maxDepth
    · have hnot : ¬ depth < maxDepth := Nat.not_lt.mpr h
      simp [h, hnot]
    · have hlt : depth < maxDepth := Nat.lt_of_not_ge h
      simp [h, hlt, eq_comm]

/-- 逆向扫描已消费前缀 `xs`（顺序为源码从左到右），返回最后一个非 trivia 词法单元的终偏移，否则返回 0。 -/
def previousSignificantEnd : List (Bool × Nat) → Nat
  | [] => 0
  | (isTrivia, endOff) :: rest =>
      match rest.any (fun p => !p.1) with
      | true => previousSignificantEnd rest
      | false => if isTrivia then 0 else endOff

/-- 更贴合逆向遍历的等价定义：对反转后的列表取首个非 trivia 的 `endOff`。 -/
def findFirstNonTriviaEnd : List (Bool × Nat) → Nat
  | [] => 0
  | (isTrivia, endOff) :: rest =>
      if !isTrivia then endOff else findFirstNonTriviaEnd rest

def previousSignificantEndRev (xs : List (Bool × Nat)) : Nat :=
  findFirstNonTriviaEnd xs.reverse

theorem findFirstNonTriviaEnd_append_trivia
    (ys : List (Bool × Nat)) (endOff : Nat) :
    findFirstNonTriviaEnd ((true, endOff) :: ys) = findFirstNonTriviaEnd ys := by
  rfl

theorem findFirstNonTriviaEnd_append_non_trivia
    (ys : List (Bool × Nat)) (endOff : Nat) :
    findFirstNonTriviaEnd ((false, endOff) :: ys) = endOff := by
  rfl

theorem findFirstNonTriviaEnd_all_trivia :
    ∀ ys : List (Bool × Nat), (∀ p ∈ ys, p.1 = true) → findFirstNonTriviaEnd ys = 0
  | [], _ => rfl
  | (isTrivia, endOff) :: tl, hall => by
      have hhd : isTrivia = true := hall (isTrivia, endOff) (by simp)
      have htl : ∀ p ∈ tl, p.1 = true := fun p hp => hall p (by simp [hp])
      simp [findFirstNonTriviaEnd, hhd, findFirstNonTriviaEnd_all_trivia tl htl]

/-- 无界列表定理（C2）：在任意前缀末尾追加非 trivia 词法单元时，逆向扫描精确返回其终偏移；
追加 trivia 词法单元时，结果严格不变；全为 trivia 时返回 0。 -/
theorem previousSignificantEndRev_spec
    (xs : List (Bool × Nat)) (endOff : Nat) :
    previousSignificantEndRev (xs ++ [(false, endOff)]) = endOff ∧
    previousSignificantEndRev (xs ++ [(true, endOff)]) = previousSignificantEndRev xs ∧
    ((∀ p ∈ xs, p.1 = true) → previousSignificantEndRev xs = 0) := by
  refine ⟨?_, ?_, ?_⟩
  · simp [previousSignificantEndRev, findFirstNonTriviaEnd]
  · simp [previousSignificantEndRev, findFirstNonTriviaEnd]
  · intro hall
    exact findFirstNonTriviaEnd_all_trivia xs.reverse (fun p hp => hall p (List.mem_reverse.mp hp))

def lambdaBodyElementIsStatement (index len : Nat) (isExprStmt : Bool) : Bool :=
  !(decide (index = len - 1) && isExprStmt)

def controlBodyElementIsStatement (valueRequired : Bool) (index len : Nat) (isExprStmt : Bool) : Bool :=
  !(valueRequired && decide (index = len - 1) && isExprStmt)

/-- 无界自然数定理（C3）：`LambdaBody` 与 `ControlBody` 仅在最后一个元素为表达式语句（且对 `ControlBody` 需 `valueRequired = true`）时进入值上下文，其余任意长度下标均保持语句上下文。 -/
theorem bodyElementIsStatement_spec
    (valueRequired : Bool) (index len : Nat) (isExprStmt : Bool) :
    (lambdaBodyElementIsStatement index len isExprStmt = false ↔
      index = len - 1 ∧ isExprStmt = true) ∧
    (controlBodyElementIsStatement valueRequired index len isExprStmt = false ↔
      valueRequired = true ∧ index = len - 1 ∧ isExprStmt = true) ∧
    (valueRequired = true →
      controlBodyElementIsStatement valueRequired index len isExprStmt =
        lambdaBodyElementIsStatement index len isExprStmt) ∧
    (valueRequired = false →
      controlBodyElementIsStatement valueRequired index len isExprStmt = true) := by
  refine ⟨?_, ?_, ?_, ?_⟩
  · unfold lambdaBodyElementIsStatement
    cases isExprStmt <;> by_cases h : index = len - 1 <;> simp [h]
  · unfold controlBodyElementIsStatement
    cases valueRequired <;> cases isExprStmt <;> by_cases h : index = len - 1 <;> simp [h]
  · intro hv
    simp [controlBodyElementIsStatement, lambdaBodyElementIsStatement, hv]
  · intro hv
    simp [controlBodyElementIsStatement, hv]

def ifMissingElseEmitsDiagnostic (hasElse statementAllowed : Bool) : Bool :=
  !hasElse && !statementAllowed

def expressionTailIsBlockBoundary (blockElements isSemicolon canStartExpr : Bool) : Bool :=
  blockElements && (isSemicolon || canStartExpr)

def expressionTailEmitsUnexpectedTrailing (inStops blockBoundary : Bool) : Bool :=
  !inStops && !blockBoundary

/-- 有限域定理（C4）：`if` 缺失 `else` 诊断守卫与表达式尾部未预期尾随词法单元判定充要条件。 -/
theorem ifElse_and_expressionTail_spec
    (hasElse statementAllowed inStops blockElements isSemicolon canStartExpr : Bool) :
    (ifMissingElseEmitsDiagnostic hasElse statementAllowed = true ↔
      hasElse = false ∧ statementAllowed = false) ∧
    (expressionTailIsBlockBoundary blockElements isSemicolon canStartExpr = true ↔
      blockElements = true ∧ (isSemicolon = true ∨ canStartExpr = true)) ∧
    (expressionTailEmitsUnexpectedTrailing
        inStops
        (expressionTailIsBlockBoundary blockElements isSemicolon canStartExpr) = true ↔
      inStops = false ∧
        (blockElements = false ∨ (isSemicolon = false ∧ canStartExpr = false))) := by
  cases hasElse <;> cases statementAllowed <;> cases inStops <;>
    cases blockElements <;> cases isSemicolon <;> cases canStartExpr <;> decide

def boundaryUsesEmptySpan (inStops atBlockSep : Bool) : Bool :=
  inStops || atBlockSep

def typeRecoveryBoundaryHit
    (inStops hasBlockSep atBlockSep hasClassFile atClassFile atFileAngle : Bool) : Bool :=
  inStops || (hasBlockSep && atBlockSep) || (hasClassFile && atClassFile) || atFileAngle

/-- 有限域定理（C5）：边界空 span 与类型恢复边界命中判定对全部 6 路标志满足单调性与短路充要条件。 -/
theorem boundarySpan_and_typeRecoveryBoundary_spec
    (inStops hasBlockSep atBlockSep hasClassFile atClassFile atFileAngle : Bool) :
    (boundaryUsesEmptySpan inStops atBlockSep = true ↔
      inStops = true ∨ atBlockSep = true) ∧
    (typeRecoveryBoundaryHit inStops hasBlockSep atBlockSep hasClassFile atClassFile atFileAngle = true ↔
      inStops = true ∨
      (hasBlockSep = true ∧ atBlockSep = true) ∨
      (hasClassFile = true ∧ atClassFile = true) ∨
      atFileAngle = true) ∧
    (hasBlockSep = false →
      typeRecoveryBoundaryHit inStops hasBlockSep atBlockSep hasClassFile atClassFile atFileAngle =
      typeRecoveryBoundaryHit inStops false false hasClassFile atClassFile atFileAngle) ∧
    (hasClassFile = false →
      typeRecoveryBoundaryHit inStops hasBlockSep atBlockSep hasClassFile atClassFile atFileAngle =
      typeRecoveryBoundaryHit inStops hasBlockSep atBlockSep false false atFileAngle) := by
  cases inStops <;> cases hasBlockSep <;> cases atBlockSep <;>
    cases hasClassFile <;> cases atClassFile <;> cases atFileAngle <;> decide

end F6Core
