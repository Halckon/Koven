namespace F6Declaration

/-- Recovery action when `const` is not followed by `val`. -/
inductive ConstValRecoveryAction where
  | missingAtBoundary
  | consumeErrorVar
  | missingKeepCurrent
  | consumeErrorOther
  deriving DecidableEq, Repr

/-- Function return form 5-way dispatch in `declaration.rs`. -/
inductive FunctionFormDispatch where
  | explicitColon
  | missingReturnTypeBeforeEqual
  | implicitUnitBlock
  | missingColonBeforeTypeRef
  | implicitUnitAbsent
  deriving DecidableEq, Repr

/-- Parameter mode lookahead context. -/
inductive ParameterModeContext where
  | namedParameter
  | functionType
  deriving DecidableEq, Repr

/-- Value parameter type-position 3-way dispatch. -/
inductive ValueParamTypeAction where
  | emptyErrorAtBoundary
  | parseTypeRef
  | recoverErrorRegion
  deriving DecidableEq, Repr

/-- Abstract token classes relevant to `declaration.rs`. -/
inductive DeclToken where
  | kwVar
  | ident
  | stringStart
  | symColon
  | symEqual
  | symLeftBrace
  | symLeftParen
  | symRightParen
  | symComma
  | eof
  | other
  deriving DecidableEq, Repr

def constValRecoveryAction (tok : DeclToken) (boundary : Bool) : ConstValRecoveryAction :=
  if boundary then
    .missingAtBoundary
  else
    match tok with
    | .kwVar => .consumeErrorVar
    | .ident | .stringStart | .symColon | .symEqual => .missingKeepCurrent
    | _ => .consumeErrorOther

def functionFormDispatch (tok : DeclToken) (canStartTypeRef : Bool) : FunctionFormDispatch :=
  match tok with
  | .symColon => .explicitColon
  | .symEqual => .missingReturnTypeBeforeEqual
  | .symLeftBrace => .implicitUnitBlock
  | _ => if canStartTypeRef then .missingColonBeforeTypeRef else .implicitUnitAbsent

def isParameterModeFollow (ctx : ParameterModeContext) (next1 : DeclToken) : Bool :=
  match next1 with
  | .ident => true
  | .symLeftParen => ctx == .functionType
  | _ => false

def valueParamTypeAction (tok : DeclToken) (canStartTypeRef : Bool) : ValueParamTypeAction :=
  match tok with
  | .symEqual | .symComma | .symRightParen | .eof => .emptyErrorAtBoundary
  | _ => if canStartTypeRef then .parseTypeRef else .recoverErrorRegion

def valueParamDefaultEmitsDiagnostic
    (sawEqualBeforeType colonIsEmpty currentIsEqualAfterType : Bool) : Bool :=
  let missingColonDefault := sawEqualBeforeType && colonIsEmpty
  currentIsEqualAfterType && !missingColonDefault

def declarationTailEmitsUnexpectedTrailing
    (tok : DeclToken) (implicitFunction isTrailingPoison : Bool) : Bool :=
  tok != .eof && (!implicitFunction || !isTrailingPoison)

/-- D1: Complete 4-way specification of `constValRecoveryAction`. -/
theorem constValRecoveryAction_spec (tok : DeclToken) (boundary : Bool) :
    (constValRecoveryAction tok boundary = .missingAtBoundary ↔ boundary = true) ∧
    (constValRecoveryAction tok boundary = .consumeErrorVar ↔
      boundary = false ∧ tok = .kwVar) ∧
    (constValRecoveryAction tok boundary = .missingKeepCurrent ↔
      boundary = false ∧
        (tok = .ident ∨ tok = .stringStart ∨ tok = .symColon ∨ tok = .symEqual)) ∧
    (constValRecoveryAction tok boundary = .consumeErrorOther ↔
      boundary = false ∧ tok ≠ .kwVar ∧ tok ≠ .ident ∧ tok ≠ .stringStart ∧
        tok ≠ .symColon ∧ tok ≠ .symEqual) := by
  cases boundary <;> cases tok <;> simp [constValRecoveryAction]

/-- D2: Complete 5-way specification of `functionFormDispatch` with punctuation priority over `canStartTypeRef`. -/
theorem functionFormDispatch_spec (tok : DeclToken) (canStartTypeRef : Bool) :
    (functionFormDispatch tok canStartTypeRef = .explicitColon ↔ tok = .symColon) ∧
    (functionFormDispatch tok canStartTypeRef = .missingReturnTypeBeforeEqual ↔ tok = .symEqual) ∧
    (functionFormDispatch tok canStartTypeRef = .implicitUnitBlock ↔ tok = .symLeftBrace) ∧
    (functionFormDispatch tok canStartTypeRef = .missingColonBeforeTypeRef ↔
      tok ≠ .symColon ∧ tok ≠ .symEqual ∧ tok ≠ .symLeftBrace ∧ canStartTypeRef = true) ∧
    (functionFormDispatch tok canStartTypeRef = .implicitUnitAbsent ↔
      tok ≠ .symColon ∧ tok ≠ .symEqual ∧ tok ≠ .symLeftBrace ∧ canStartTypeRef = false) := by
  cases tok <;> cases canStartTypeRef <;> simp [functionFormDispatch]

/-- D3: Exact iff characterization of `isParameterModeFollow`. -/
theorem parameterModeFollow_spec (ctx : ParameterModeContext) (next1 : DeclToken) :
    isParameterModeFollow ctx next1 = true ↔
      next1 = .ident ∨ (ctx = .functionType ∧ next1 = .symLeftParen) := by
  cases ctx <;> cases next1 <;> simp [isParameterModeFollow]

/-- D4: Complete 3-way specification of `valueParamTypeAction` and duplicate-diagnostic suppression for parameter defaults. -/
theorem valueParamTypeAction_and_defaultGuard_spec
    (tok : DeclToken) (canStartTypeRef sawEqualBeforeType colonIsEmpty currentIsEqualAfterType : Bool) :
    (valueParamTypeAction tok canStartTypeRef = .emptyErrorAtBoundary ↔
      tok = .symEqual ∨ tok = .symComma ∨ tok = .symRightParen ∨ tok = .eof) ∧
    (valueParamTypeAction tok canStartTypeRef = .parseTypeRef ↔
      tok ≠ .symEqual ∧ tok ≠ .symComma ∧ tok ≠ .symRightParen ∧ tok ≠ .eof ∧
        canStartTypeRef = true) ∧
    (valueParamTypeAction tok canStartTypeRef = .recoverErrorRegion ↔
      tok ≠ .symEqual ∧ tok ≠ .symComma ∧ tok ≠ .symRightParen ∧ tok ≠ .eof ∧
        canStartTypeRef = false) ∧
    (valueParamDefaultEmitsDiagnostic sawEqualBeforeType colonIsEmpty currentIsEqualAfterType = true ↔
      currentIsEqualAfterType = true ∧ ¬(sawEqualBeforeType = true ∧ colonIsEmpty = true)) := by
  refine ⟨?_, ?_, ?_, ?_⟩
  · cases tok <;> cases canStartTypeRef <;> simp [valueParamTypeAction]
  · cases tok <;> cases canStartTypeRef <;> simp [valueParamTypeAction]
  · cases tok <;> cases canStartTypeRef <;> simp [valueParamTypeAction]
  · cases sawEqualBeforeType <;> cases colonIsEmpty <;> cases currentIsEqualAfterType <;>
      simp [valueParamDefaultEmitsDiagnostic]

/-- D5: `consume_declaration_tail` suppresses `UNEXPECTED_TRAILING_TOKEN` iff at EOF or on an implicit-unit function trailing poison. -/
theorem declarationTailEmits_spec (tok : DeclToken) (implicitFunction isTrailingPoison : Bool) :
    declarationTailEmitsUnexpectedTrailing tok implicitFunction isTrailingPoison = false ↔
      tok = .eof ∨ (implicitFunction = true ∧ isTrailingPoison = true) := by
  cases tok <;> cases implicitFunction <;> cases isTrailingPoison <;>
    simp [declarationTailEmitsUnexpectedTrailing]

end F6Declaration
