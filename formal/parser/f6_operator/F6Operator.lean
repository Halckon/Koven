/-!
# F6 Slice 22: `operator.rs` 运算符与表达式起始纯内核机器证明模型

本文件对应 `crates/lang-frontend/src/parser/engine/operator.rs` 中的 6 个纯判定内核：
1. `is_unsupported_adjacent_operator_pair`（相邻非法双符号 `++`/`--`/`<<`/`>>`/`...`）
2. `can_start_expression_kind` / `can_start_type_ref_kind` / `is_poison_lexeme_kind`
3. `is_declaration_trailing_poison_decision`（声明尾随毒化判定）
4. `infix_rule_from_parts`（35 个中缀运算符形式——28 个符号/硬关键字 + 7 个软关键字——的优先级、左右结合性、非结合组与类型右操作数）

证据等级：`proved (model, complete finite domain)`（5 条有限枚举/布尔全空间定理，零 Mathlib，仅核心公理）。
-/

namespace F6Operator

inductive OpToken where
  | ident
  | literalOther
  | stringStart
  | exprKeyword
  | kwIn
  | kwIs
  | kwAs
  | symLeftParen
  | symLeftBrace
  | symColonColon
  | symBang
  | symPlus
  | symMinus
  | symStar
  | symSlash
  | symPercent
  | symDotDot
  | symDotDotLess
  | symDot
  | symQuestionColon
  | symBangIn
  | symBangIs
  | symLess
  | symGreater
  | symLessEqual
  | symGreaterEqual
  | symEqualEqual
  | symBangEqual
  | symAndAnd
  | symOrOr
  | symAsQuestion
  | symEqual
  | symCompoundAssign
  | invalid
  | reservedWord
  | other
  deriving DecidableEq, Repr

inductive SoftInfixKeyword where
  | kwTo
  | kwShl
  | kwShr
  | kwUshr
  | kwAnd
  | kwXor
  | kwOr
  deriving DecidableEq, Repr

inductive NonAssociativeGroup where
  | range
  | membership
  | comparison
  | equality
  deriving DecidableEq, Repr

inductive InfixKindClass where
  | binaryOther
  | binaryElvis
  | cast
  | typeTest
  | assignment
  deriving DecidableEq, Repr

structure InfixRule where
  prec : Nat
  rightPrec : Nat
  kind : InfixKindClass
  nonAssocGroup : Option NonAssociativeGroup
  deriving DecidableEq, Repr

def mkLeft (prec : Nat) (kind : InfixKindClass) : InfixRule :=
  { prec := prec, rightPrec := prec + 1, kind := kind, nonAssocGroup := none }

def mkRight (prec : Nat) (kind : InfixKindClass) : InfixRule :=
  { prec := prec, rightPrec := prec, kind := kind, nonAssocGroup := none }

def mkNonAssoc (prec : Nat) (kind : InfixKindClass) (group : NonAssociativeGroup) : InfixRule :=
  { prec := prec, rightPrec := prec + 1, kind := kind, nonAssocGroup := some group }

def isUnsupportedAdjacentOperatorPair (first second : OpToken) (adjacent : Bool) : Bool :=
  adjacent &&
    match first, second with
    | .symPlus, .symPlus => true
    | .symMinus, .symMinus => true
    | .symLess, .symLess => true
    | .symGreater, .symGreater => true
    | .symDotDot, .symDot => true
    | _, _ => false

def isPoisonKind (tok : OpToken) : Bool :=
  match tok with
  | .invalid | .reservedWord => true
  | _ => false

def canStartTypeRef (tok : OpToken) : Bool :=
  match tok with
  | .ident | .symLeftParen => true
  | _ => false

def canStartExpression (tok : OpToken) : Bool :=
  match tok with
  | .ident | .literalOther | .stringStart | .exprKeyword
  | .symLeftParen | .symLeftBrace | .symColonColon | .symBang | .symPlus | .symMinus
  | .invalid | .reservedWord => true
  | _ => false

def isDeclarationTrailingPoison
    (tok : OpToken)
    (hasStringRecovery hasLexicalPoisonStringRecovery : Bool) : Bool :=
  isPoisonKind tok ||
    (tok == .stringStart && (hasStringRecovery || hasLexicalPoisonStringRecovery))

def infixRule (tok : OpToken) (soft : Option SoftInfixKeyword) : Option InfixRule :=
  match tok with
  | .symStar | .symSlash | .symPercent => some (mkLeft 15 .binaryOther)
  | .symPlus | .symMinus => some (mkLeft 14 .binaryOther)
  | .symDotDot | .symDotDotLess => some (mkNonAssoc 9 .binaryOther .range)
  | .ident =>
      match soft with
      | some .kwTo => some (mkLeft 8 .binaryOther)
      | some .kwShl | some .kwShr | some .kwUshr => some (mkLeft 13 .binaryOther)
      | some .kwAnd => some (mkLeft 12 .binaryOther)
      | some .kwXor => some (mkLeft 11 .binaryOther)
      | some .kwOr => some (mkLeft 10 .binaryOther)
      | none => none
  | .symQuestionColon => some (mkRight 7 .binaryElvis)
  | .kwIn | .symBangIn => some (mkNonAssoc 6 .binaryOther .membership)
  | .kwIs | .symBangIs => some (mkNonAssoc 6 .typeTest .membership)
  | .symLess | .symGreater | .symLessEqual | .symGreaterEqual =>
      some (mkNonAssoc 5 .binaryOther .comparison)
  | .symEqualEqual | .symBangEqual => some (mkNonAssoc 4 .binaryOther .equality)
  | .symAndAnd => some (mkLeft 3 .binaryOther)
  | .symOrOr => some (mkLeft 2 .binaryOther)
  | .kwAs | .symAsQuestion => some (mkLeft 16 .cast)
  | .symEqual | .symCompoundAssign => some (mkRight 1 .assignment)
  | _ => none

def isTypeOperandInfix (kind : InfixKindClass) : Bool :=
  match kind with
  | .cast | .typeTest => true
  | _ => false

theorem unsupportedAdjacentOperatorPair_spec
    (first second : OpToken) (adjacent : Bool) :
    isUnsupportedAdjacentOperatorPair first second adjacent = true ↔
      adjacent = true ∧
        ((first = .symPlus ∧ second = .symPlus) ∨
         (first = .symMinus ∧ second = .symMinus) ∨
         (first = .symLess ∧ second = .symLess) ∨
         (first = .symGreater ∧ second = .symGreater) ∨
         (first = .symDotDot ∧ second = .symDot)) := by
  cases first <;> cases second <;> cases adjacent <;> decide

theorem starter_and_poison_hierarchy_spec (tok : OpToken) :
    (canStartTypeRef tok = true → canStartExpression tok = true) ∧
    (isPoisonKind tok = true → canStartExpression tok = true) ∧
    (isPoisonKind tok = true → canStartTypeRef tok = false) ∧
    (isPoisonKind tok = true ↔ tok = .invalid ∨ tok = .reservedWord) ∧
    (canStartTypeRef tok = true ↔ tok = .ident ∨ tok = .symLeftParen) := by
  cases tok <;> decide

theorem declarationTrailingPoison_spec
    (tok : OpToken) (hasString hasLexicalPoison : Bool) :
    isDeclarationTrailingPoison tok hasString hasLexicalPoison = true ↔
      isPoisonKind tok = true ∨
        (tok = .stringStart ∧ (hasString = true ∨ hasLexicalPoison = true)) := by
  cases tok <;> cases hasString <;> cases hasLexicalPoison <;> decide

def infixRuleHasTypeOperand (tok : OpToken) (soft : Option SoftInfixKeyword) : Bool :=
  match infixRule tok soft with
  | some rule => isTypeOperandInfix rule.kind
  | none => false

theorem infixRule_precedence_and_associativity_spec
    (tok : OpToken) (soft : Option SoftInfixKeyword) :
    match infixRule tok soft with
    | none => True
    | some rule =>
        (1 ≤ rule.prec ∧ rule.prec < 17) ∧
        ((rule.rightPrec = rule.prec ↔ (rule.kind = .binaryElvis ∨ rule.kind = .assignment)) ∧
         (rule.rightPrec = rule.prec + 1 ↔ (rule.kind ≠ .binaryElvis ∧ rule.kind ≠ .assignment))) ∧
        (rule.nonAssocGroup.isSome = true ↔
          (rule.prec = 4 ∨ rule.prec = 5 ∨ rule.prec = 6 ∨ rule.prec = 9)) ∧
        (rule.nonAssocGroup.isSome = true → rule.rightPrec = rule.prec + 1) := by
  cases tok <;> cases soft with
  | none => simp [infixRule, mkLeft, mkRight, mkNonAssoc]
  | some kw => cases kw <;> simp [infixRule, mkLeft, mkRight, mkNonAssoc]

theorem infixRule_operand_and_soft_keyword_spec
    (tok : OpToken) (soft : Option SoftInfixKeyword) :
    (infixRuleHasTypeOperand tok soft = true ↔
      (tok = .kwAs ∨ tok = .symAsQuestion ∨ tok = .kwIs ∨ tok = .symBangIs)) ∧
    (tok = .ident → ((infixRule tok soft).isSome = true ↔ soft.isSome = true)) ∧
    (tok ≠ .ident → infixRule tok soft = infixRule tok none) ∧
    (isPoisonKind tok = true → infixRule tok soft = none) := by
  cases tok <;> cases soft with
  | none => decide
  | some kw => cases kw <;> decide

end F6Operator
