namespace F6WhenBoundary

inductive WhenEntryBody where
  | withSubject
  | withoutSubject
  deriving DecidableEq, Repr

inductive ControlToken where
  | arrow
  | elseKw
  | leftParen
  | leftBrace
  | plus
  | minus
  | isKw
  | inKw
  | bangIs
  | bangIn
  | other
  deriving DecidableEq, Repr

def isNewSelectorAfterLineBreak (body : WhenEntryBody) (tok : ControlToken) : Bool :=
  match tok with
  | .leftParen | .leftBrace | .plus | .minus => true
  | .isKw | .inKw | .bangIs | .bangIn =>
      match body with
      | .withSubject => true
      | .withoutSubject => false
  | .arrow | .elseKw | .other => false

structure SelectorScanState where
  needsOperand : Bool
  pendingElse : Nat
  deriving DecidableEq, Repr

def SelectorScanState.initial : SelectorScanState :=
  { needsOperand := false, pendingElse := 0 }

inductive SelectorEvent where
  | arrow
  | elseKw
  | newSelectorAfterLineBreak
  | nonOperandContinuation
  | nonOperandExpectsOperand
  | nonOperandGroupFallthrough
  | operandIf
  | operandPrefixContinue
  | operandComplete
  | reject
  deriving DecidableEq, Repr

inductive SelectorStep where
  | cont (state : SelectorScanState)
  | finish (accepted : Bool)
  deriving DecidableEq, Repr

def step (state : SelectorScanState) (ev : SelectorEvent) : SelectorStep :=
  if !state.needsOperand then
    match ev with
    | .arrow => .finish (decide (state.pendingElse = 0))
    | .elseKw =>
        if state.pendingElse > 0 then
          .cont { needsOperand := true, pendingElse := state.pendingElse - 1 }
        else
          .finish false
    | .newSelectorAfterLineBreak => .finish false
    | .nonOperandContinuation | .nonOperandGroupFallthrough => .cont state
    | .nonOperandExpectsOperand =>
        .cont { needsOperand := true, pendingElse := state.pendingElse }
    | .operandIf | .operandPrefixContinue | .operandComplete | .reject => .finish false
  else
    match ev with
    | .operandIf =>
        .cont { needsOperand := true, pendingElse := state.pendingElse + 1 }
    | .operandPrefixContinue => .cont state
    | .operandComplete =>
        .cont { needsOperand := false, pendingElse := state.pendingElse }
    | .arrow
    | .elseKw
    | .newSelectorAfterLineBreak
    | .nonOperandContinuation
    | .nonOperandExpectsOperand
    | .nonOperandGroupFallthrough
    | .reject => .finish false

def stepNonOperandControl
    (state : SelectorScanState)
    (body : WhenEntryBody)
    (tok : ControlToken)
    (hasLineBreak : Bool) : Option SelectorStep :=
  if state.needsOperand then
    none
  else if tok = .arrow then
    some (step state .arrow)
  else if state.pendingElse > 0 && tok = .elseKw then
    some (step state .elseKw)
  else if isNewSelectorAfterLineBreak body tok && hasLineBreak then
    some (step state .newSelectorAfterLineBreak)
  else
    none

def runPrefix (state : SelectorScanState) : List SelectorEvent → Option SelectorScanState
  | [] => some state
  | ev :: rest =>
      match step state ev with
      | .cont next => runPrefix next rest
      | .finish _ => none

def runEventsFrom (state : SelectorScanState) : List SelectorEvent → Bool
  | [] => false
  | ev :: rest =>
      match step state ev with
      | .cont next => runEventsFrom next rest
      | .finish accepted => accepted

def countIf : List SelectorEvent → Nat
  | [] => 0
  | .operandIf :: rest => 1 + countIf rest
  | _ :: rest => countIf rest

def countElse : List SelectorEvent → Nat
  | [] => 0
  | .elseKw :: rest => 1 + countElse rest
  | _ :: rest => countElse rest

/-- W1: Exact subject-sensitive characterization of `isNewSelectorAfterLineBreak`. -/
theorem isNewSelectorAfterLineBreak_spec (body : WhenEntryBody) (tok : ControlToken) :
    isNewSelectorAfterLineBreak body tok = true ↔
      (tok = .leftParen ∨ tok = .leftBrace ∨ tok = .plus ∨ tok = .minus) ∨
        (body = .withSubject ∧ (tok = .isKw ∨ tok = .inKw ∨ tok = .bangIs ∨ tok = .bangIn)) := by
  cases body <;> cases tok <;> simp [isNewSelectorAfterLineBreak]

/-- W2: `step state .arrow = .finish true` iff operand is complete and `pendingElse = 0`. -/
theorem step_arrow_iff (state : SelectorScanState) :
    step state .arrow = .finish true ↔ state.needsOperand = false ∧ state.pendingElse = 0 := by
  cases state with
  | mk needs pending =>
      cases needs <;> simp [step]

/-- W3: Exact `else` transition: decrements `pendingElse` without underflow iff `!needsOperand ∧ pendingElse > 0`, otherwise rejects. -/
theorem step_else_exact (state : SelectorScanState) :
    (state.needsOperand = false ∧ state.pendingElse > 0 →
      step state .elseKw = .cont { needsOperand := true, pendingElse := state.pendingElse - 1 }) ∧
    (state.needsOperand = true ∨ state.pendingElse = 0 →
      step state .elseKw = .finish false) := by
  cases state with
  | mk needs pending =>
      constructor
      · rintro ⟨hNeeds, hPos⟩
        simp at hNeeds hPos
        simp [step, hNeeds, hPos]
      · rintro (hNeeds | hZero)
        · simp at hNeeds
          simp [step, hNeeds]
        · simp at hZero
          cases needs <;> simp [step, hZero]

/-- W4: Exact `if` transition: increments `pendingElse` and stays `needsOperand = true` iff `needsOperand = true`, otherwise rejects. -/
theorem step_if_exact (state : SelectorScanState) :
    (state.needsOperand = true →
      step state .operandIf = .cont { needsOperand := true, pendingElse := state.pendingElse + 1 }) ∧
    (state.needsOperand = false →
      step state .operandIf = .finish false) := by
  cases state with
  | mk needs pending =>
      constructor
      · intro hNeeds
        simp at hNeeds
        simp [step, hNeeds]
      · intro hNeeds
        simp at hNeeds
        simp [step, hNeeds]

/-- W5: `stepNonOperandControl` returns `none` when expecting an operand and rejects on line-break new selectors. -/
theorem stepNonOperandControl_spec
    (state : SelectorScanState)
    (body : WhenEntryBody)
    (tok : ControlToken)
    (hasLineBreak : Bool) :
    (state.needsOperand = true →
      stepNonOperandControl state body tok hasLineBreak = none) ∧
    (state.needsOperand = false → isNewSelectorAfterLineBreak body tok = true → hasLineBreak = true →
      stepNonOperandControl state body tok hasLineBreak = some (.finish false)) := by
  cases state with
  | mk needs pending =>
      constructor
      · intro hNeeds
        simp at hNeeds
        simp [stepNonOperandControl, hNeeds]
      · intro hNeeds hNew hBreak
        simp at hNeeds
        cases body <;> cases tok <;>
          simp [isNewSelectorAfterLineBreak, stepNonOperandControl, step, hNeeds, hBreak] at hNew ⊢

/-- W6: Unbounded `if`/`else` conservation invariant along any continuing prefix. -/
theorem runPrefix_pendingElse_balance
    (state finalState : SelectorScanState)
    (pre : List SelectorEvent)
    (hRun : runPrefix state pre = some finalState) :
    finalState.pendingElse + countElse pre = state.pendingElse + countIf pre := by
  induction pre generalizing state with
  | nil =>
      simp [runPrefix] at hRun
      subst hRun
      simp [countElse, countIf]
  | cons ev rest ih =>
      simp [runPrefix] at hRun
      cases hStep : step state ev with
      | finish accepted =>
          simp [hStep] at hRun
      | cont next =>
          simp [hStep] at hRun
          have hRest := ih next hRun
          cases state with
          | mk needs pending =>
              cases needs <;> cases ev <;> simp [step] at hStep
              · split at hStep
                · rename_i hPos
                  injection hStep with hNext
                  subst hNext
                  simp [countElse, countIf] at hRest ⊢
                  omega
                · cases hStep
              · subst hStep
                simp [countElse, countIf] at hRest ⊢
                exact hRest
              · subst hStep
                simp [countElse, countIf] at hRest ⊢
                exact hRest
              · subst hStep
                simp [countElse, countIf] at hRest ⊢
                exact hRest
              · subst hStep
                simp [countElse, countIf] at hRest ⊢
                omega
              · subst hStep
                simp [countElse, countIf] at hRest ⊢
                exact hRest
              · subst hStep
                simp [countElse, countIf] at hRest ⊢
                exact hRest

/-- W7: Acceptance over an arbitrary-length event stream occurs iff an `.arrow` event is reached in a balanced, complete-operand state. -/
theorem runEventsFrom_true_iff_arrow_balanced
    (state : SelectorScanState)
    (events : List SelectorEvent) :
    runEventsFrom state events = true ↔
      ∃ pre suf finalState,
        events = pre ++ .arrow :: suf ∧
        runPrefix state pre = some finalState ∧
        finalState.needsOperand = false ∧
        finalState.pendingElse = 0 := by
  induction events generalizing state with
  | nil =>
      constructor
      · intro h
        simp [runEventsFrom] at h
      · rintro ⟨pre, suf, _, hEq, _⟩
        cases pre <;> simp at hEq
  | cons ev rest ih =>
      constructor
      · intro h
        simp [runEventsFrom] at h
        cases hStep : step state ev with
        | finish accepted =>
            simp [hStep] at h
            subst h
            have hArrow : ev = .arrow ∧ state.needsOperand = false ∧ state.pendingElse = 0 := by
              cases state with
              | mk needs pending =>
                  cases needs <;> cases ev <;> simp [step] at hStep
                  · exact ⟨rfl, rfl, hStep⟩
                  · split at hStep <;> simp at hStep
            rcases hArrow with ⟨rfl, hNeeds, hZero⟩
            exact ⟨[], rest, state, rfl, rfl, hNeeds, hZero⟩
        | cont next =>
            simp [hStep] at h
            rcases (ih next).mp h with ⟨pre, suf, finalState, hEq, hPre, hNeeds, hZero⟩
            refine ⟨ev :: pre, suf, finalState, ?_, ?_, hNeeds, hZero⟩
            · simp [hEq]
            · simp [runPrefix, hStep, hPre]
      · rintro ⟨pre, suf, finalState, hEq, hPre, hNeeds, hZero⟩
        cases pre with
        | nil =>
            simp at hEq
            rcases hEq with ⟨rfl, rfl⟩
            simp [runPrefix] at hPre
            subst hPre
            simp [runEventsFrom, step, hNeeds, hZero]
        | cons ev' pre' =>
            simp at hEq
            rcases hEq with ⟨rfl, rfl⟩
            simp [runPrefix] at hPre
            cases hStep : step state ev with
            | finish _ =>
                simp [hStep] at hPre
            | cont next =>
                simp [hStep] at hPre
                simp [runEventsFrom, hStep]
                exact (ih next).mpr ⟨pre', suf, finalState, rfl, hPre, hNeeds, hZero⟩

end F6WhenBoundary
