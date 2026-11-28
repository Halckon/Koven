namespace F6LambdaBody

inductive LambdaLexemeKind where
  | rightBrace
  | semicolon
  | comma
  | arrow
  | kwConst
  | kwVal
  | other
  deriving DecidableEq, Repr

inductive LambdaBodyLoopAction where
  | stop
  | skipSemicolon
  | parseElement
  deriving DecidableEq, Repr

def lambdaBodyLoopAction (kind : LambdaLexemeKind) (outerHardStop : Bool) : LambdaBodyLoopAction :=
  if kind == .rightBrace || outerHardStop then
    .stop
  else if kind == .semicolon then
    .skipSemicolon
  else
    .parseElement

def isLambdaBodyUnsupportedSymbol (kind : LambdaLexemeKind) : Bool :=
  kind == .comma || kind == .arrow

def missingCloserEmitsDiagnostic
    (currentKind : LambdaLexemeKind)
    (terminalErrorAtEof isPoison : Bool) : Bool :=
  currentKind != .rightBrace && !terminalErrorAtEof && !isPoison

def unsupportedConsumesVal
    (firstKind : LambdaLexemeKind)
    (secondKind : Option LambdaLexemeKind) : Bool :=
  firstKind == .kwConst && secondKind == some .kwVal

def isSkippableSemicolon (entry : LambdaLexemeKind × Bool) : Bool :=
  lambdaBodyLoopAction entry.1 entry.2 == .skipSemicolon

def skipLambdaBodySemicolons : List (LambdaLexemeKind × Bool) → Nat
  | [] => 0
  | entry :: rest =>
      if isSkippableSemicolon entry then
        1 + skipLambdaBodySemicolons rest
      else
        0

/-- L1 (finite domain): Exact characterization of `lambdaBodyLoopAction`. -/
theorem lambdaBodyLoopAction_spec
    (kind : LambdaLexemeKind)
    (outerHardStop : Bool) :
    (lambdaBodyLoopAction kind outerHardStop = .stop ↔
      kind = .rightBrace ∨ outerHardStop = true) ∧
    (lambdaBodyLoopAction kind outerHardStop = .skipSemicolon ↔
      kind = .semicolon ∧ outerHardStop = false) ∧
    (lambdaBodyLoopAction kind outerHardStop = .parseElement ↔
      kind ≠ .rightBrace ∧ kind ≠ .semicolon ∧ outerHardStop = false) := by
  cases kind <;> cases outerHardStop <;> simp [lambdaBodyLoopAction]

/-- L2 (finite domain): Exact characterization of `isLambdaBodyUnsupportedSymbol`. -/
theorem isLambdaBodyUnsupportedSymbol_spec (kind : LambdaLexemeKind) :
    isLambdaBodyUnsupportedSymbol kind = true ↔ kind = .comma ∨ kind = .arrow := by
  cases kind <;> simp [isLambdaBodyUnsupportedSymbol]

/-- L3 (finite domain): Exact characterization of missing-closer diagnostic guard. -/
theorem missingCloserEmitsDiagnostic_spec
    (currentKind : LambdaLexemeKind)
    (terminalErrorAtEof isPoison : Bool) :
    missingCloserEmitsDiagnostic currentKind terminalErrorAtEof isPoison = true ↔
      currentKind ≠ .rightBrace ∧ terminalErrorAtEof = false ∧ isPoison = false := by
  cases currentKind <;> cases terminalErrorAtEof <;> cases isPoison <;>
    simp [missingCloserEmitsDiagnostic]

/-- L4 (finite domain): Exact characterization of `const val` two-token consumption in unsupported lambda body forms. -/
theorem unsupportedConsumesVal_spec
    (firstKind : LambdaLexemeKind)
    (secondKind : Option LambdaLexemeKind) :
    unsupportedConsumesVal firstKind secondKind = true ↔
      firstKind = .kwConst ∧ secondKind = some .kwVal := by
  cases firstKind <;> cases secondKind with
  | none => simp [unsupportedConsumesVal]
  | some s => cases s <;> simp [unsupportedConsumesVal]

/-- L5 (unbounded): `skipLambdaBodySemicolons` returns a bounded index `k ≤ xs.length` where every earlier entry is a skippable semicolon and entry `k` (if any) is not. -/
theorem skipLambdaBodySemicolons_stops_at_non_semicolon
    (xs : List (LambdaLexemeKind × Bool)) :
    skipLambdaBodySemicolons xs ≤ xs.length ∧
    (∀ i, i < skipLambdaBodySemicolons xs →
      ∃ e, xs[i]? = some e ∧ isSkippableSemicolon e = true) ∧
    (∀ e, xs[skipLambdaBodySemicolons xs]? = some e →
      isSkippableSemicolon e = false) := by
  induction xs with
  | nil =>
      refine ⟨Nat.zero_le _, ?_, ?_⟩
      · intro i hi
        simp [skipLambdaBodySemicolons] at hi
      · intro e he
        simp [skipLambdaBodySemicolons] at he
  | cons hd rest ih =>
      rcases ih with ⟨ihLe, ihAll, ihStop⟩
      by_cases hSkip : isSkippableSemicolon hd = true
      · refine ⟨?_, ?_, ?_⟩
        · simp [skipLambdaBodySemicolons, hSkip]
          omega
        · intro i hi
          cases i with
          | zero =>
              exact ⟨hd, by simp, hSkip⟩
          | succ j =>
              have hj : j < skipLambdaBodySemicolons rest := by
                simp [skipLambdaBodySemicolons, hSkip] at hi
                omega
              simpa using ihAll j hj
        · intro e he
          simp [skipLambdaBodySemicolons, hSkip, Nat.add_comm] at he
          exact ihStop e he
      · have hFalse : isSkippableSemicolon hd = false := by
          cases hVal : isSkippableSemicolon hd with
          | false => rfl
          | true => exact False.elim (hSkip hVal)
        refine ⟨?_, ?_, ?_⟩
        · simp [skipLambdaBodySemicolons, hFalse]
        · intro i hi
          simp [skipLambdaBodySemicolons, hFalse] at hi
        · intro e he
          simp [skipLambdaBodySemicolons, hFalse] at he
          subst he
          exact hFalse

/-- L6 (unbounded): Prepending any number of non-hard-stop semicolons shifts `skipLambdaBodySemicolons` by exactly `m`. -/
theorem skipLambdaBodySemicolons_prepend_semicolons
    (m : Nat)
    (xs : List (LambdaLexemeKind × Bool)) :
    skipLambdaBodySemicolons (List.replicate m (.semicolon, false) ++ xs) =
      m + skipLambdaBodySemicolons xs := by
  induction m with
  | zero => simp
  | succ k ih =>
      simp [List.replicate, skipLambdaBodySemicolons, isSkippableSemicolon,
            lambdaBodyLoopAction, ih, Nat.succ_add]

end F6LambdaBody
