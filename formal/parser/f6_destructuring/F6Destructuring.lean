namespace F6Destructuring

inductive DestructuringLexemeKind where
  | eof
  | kwVal
  | kwVar
  | kwConst
  | leftParen
  | rightParen
  | leftBrace
  | equal
  | comma
  | other
  deriving DecidableEq, Repr

inductive DestructuringEvent where
  | emptyComma
  | binding
  | separatorComma
  | missingSeparatorIdent
  deriving DecidableEq, Repr

inductive DestructuringExitKind where
  | rightParen
  | boundary
  deriving DecidableEq, Repr

structure DestructuringLoopState where
  hasBindings : Bool := false
  commaAlreadyReported : Bool := false
  needsBinding : Bool := true
  deriving DecidableEq, Repr

def isLocalDestructuringStart
    (expectedKw first : DestructuringLexemeKind)
    (second : Option DestructuringLexemeKind) : Bool :=
  first == expectedKw && second == some .leftParen

def isConstLocalDestructuringStart
    (first : DestructuringLexemeKind)
    (second third : Option DestructuringLexemeKind) : Bool :=
  first == .kwConst && second == some .kwVal && third == some .leftParen

def exitEmitsMissingBinding
    (exitKind : DestructuringExitKind)
    (st : DestructuringLoopState) : Bool :=
  match exitKind with
  | .rightParen => !st.hasBindings
  | .boundary => st.needsBinding && !st.commaAlreadyReported

def stepDestructuringLoop
    (st : DestructuringLoopState)
    (ev : DestructuringEvent) : DestructuringLoopState :=
  match ev with
  | .emptyComma =>
      { hasBindings := true, commaAlreadyReported := true, needsBinding := true }
  | .binding =>
      { hasBindings := true, commaAlreadyReported := false, needsBinding := false }
  | .separatorComma =>
      { st with needsBinding := true }
  | .missingSeparatorIdent =>
      { st with needsBinding := true }

def runDestructuringLoop
    (st : DestructuringLoopState) : List DestructuringEvent → DestructuringLoopState
  | [] => st
  | ev :: rest => runDestructuringLoop (stepDestructuringLoop st ev) rest

def loopInvariant (st : DestructuringLoopState) : Prop :=
  st.commaAlreadyReported = true → st.needsBinding = true ∧ st.hasBindings = true

def initializerMissingDiagnostic
    (hasEquals boundary startsPrimary terminalLexerRoot isPoison : Bool) : Bool :=
  hasEquals && boundary && !startsPrimary && !terminalLexerRoot && !isPoison

/-- D1: Exact characterization of `isLocalDestructuringStart`. -/
theorem isLocalDestructuringStart_spec
    (expectedKw first : DestructuringLexemeKind)
    (second : Option DestructuringLexemeKind) :
    isLocalDestructuringStart expectedKw first second = true ↔
      first = expectedKw ∧ second = some .leftParen := by
  unfold isLocalDestructuringStart
  cases second with
  | none => simp
  | some s => cases s <;> simp

/-- D2: Exact characterization of `isConstLocalDestructuringStart`. -/
theorem isConstLocalDestructuringStart_spec
    (first : DestructuringLexemeKind)
    (second third : Option DestructuringLexemeKind) :
    isConstLocalDestructuringStart first second third = true ↔
      first = .kwConst ∧ second = some .kwVal ∧ third = some .leftParen := by
  unfold isConstLocalDestructuringStart
  cases first <;> cases second with
  | none => simp
  | some s =>
      cases s <;> cases third with
      | none => simp
      | some t => cases t <;> simp

/-- D3: Exact characterization of missing-binding diagnostics on loop exit. -/
theorem exitEmitsMissingBinding_spec
    (exitKind : DestructuringExitKind)
    (st : DestructuringLoopState) :
    (exitKind = .rightParen →
      (exitEmitsMissingBinding exitKind st = true ↔ st.hasBindings = false)) ∧
    (exitKind = .boundary →
      (exitEmitsMissingBinding exitKind st = true ↔
        st.needsBinding = true ∧ st.commaAlreadyReported = false)) := by
  cases st with
  | mk hb cr nb =>
      cases exitKind <;> cases hb <;> cases cr <;> cases nb <;> simp [exitEmitsMissingBinding]

/-- D4: Single-step monotonicity of `hasBindings` and preservation of `loopInvariant`. -/
theorem stepDestructuringLoop_preserves_invariant
    (st : DestructuringLoopState)
    (ev : DestructuringEvent)
    (hInv : loopInvariant st) :
    (st.hasBindings = true → (stepDestructuringLoop st ev).hasBindings = true) ∧
    loopInvariant (stepDestructuringLoop st ev) := by
  cases st with
  | mk hb cr nb =>
      cases ev <;> cases hb <;> cases cr <;> cases nb <;>
        simp [stepDestructuringLoop, loopInvariant] at hInv ⊢

/-- D5: Unbounded preservation: `runDestructuringLoop` preserves `loopInvariant` over arbitrary-length event sequences, preventing double missing-binding reports after an empty comma. -/
theorem runDestructuringLoop_preserves_invariant
    (events : List DestructuringEvent)
    (st : DestructuringLoopState)
    (hInv : loopInvariant st) :
    loopInvariant (runDestructuringLoop st events) ∧
    ((runDestructuringLoop st events).commaAlreadyReported = true →
      exitEmitsMissingBinding .rightParen (runDestructuringLoop st events) = false ∧
      exitEmitsMissingBinding .boundary (runDestructuringLoop st events) = false) := by
  have hPres : loopInvariant (runDestructuringLoop st events) := by
    induction events generalizing st with
    | nil => exact hInv
    | cons ev rest ih =>
        exact ih (stepDestructuringLoop st ev)
          (stepDestructuringLoop_preserves_invariant st ev hInv).2
  refine ⟨hPres, ?_⟩
  intro hCr
  have ⟨hNb, hHb⟩ := hPres hCr
  simp [exitEmitsMissingBinding, hCr, hHb]

/-- D6: Exact characterization of `initializerMissingDiagnostic`. -/
theorem initializerMissingDiagnostic_spec
    (hasEquals boundary startsPrimary terminalLexerRoot isPoison : Bool) :
    initializerMissingDiagnostic hasEquals boundary startsPrimary terminalLexerRoot isPoison = true ↔
      hasEquals = true ∧ boundary = true ∧ startsPrimary = false ∧
        terminalLexerRoot = false ∧ isPoison = false := by
  cases hasEquals <;> cases boundary <;> cases startsPrimary <;>
    cases terminalLexerRoot <;> cases isPoison <;>
    simp [initializerMissingDiagnostic]

end F6Destructuring
