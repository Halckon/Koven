/-
  SPEC-0498 (F6 third slice): Machine-checked unbounded model proofs for
  `GroupOwners::outside_lexical_symbol` lexical-depth preservation, terminal-event
  monotonicity, and top-level delimiter isolation
  (`crates/lang-frontend/src/parser/engine/recovery.rs`).

  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, unbounded)` -- NOT full-input production G4.
  Paired with Kani direct-Rust harness in `group_owner_kani.rs`.
-/

namespace F6GroupOwners

inductive OwnerKind where
  | string
  | interpolation
  deriving DecidableEq, Repr

inductive LexemeKind where
  | stringStart
  | interpolationStart
  | stringEnd
  | interpolationEnd
  | symbol (sym : Nat)
  | other
  deriving DecidableEq, Repr

structure Lexeme where
  startOffset : Nat
  kind : LexemeKind
  deriving DecidableEq, Repr

structure TerminalEvent where
  offset : Nat
  kind : OwnerKind
  deriving DecidableEq, Repr

structure OwnerState where
  nextTerminal : Nat := 0
  lexicalDepth : Nat := 0
  deriving DecidableEq, Repr


/-- Exact functional model of the `while terminals.get(next_terminal).is_some_and(...)` loop. -/
def drainLoop : List TerminalEvent → Nat → Nat → Nat → Nat × Nat
  | [], _, nextT, depth => (nextT, depth)
  | ev :: rest, startOffset, nextT, depth =>
    if ev.offset ≤ startOffset then
      drainLoop rest startOffset (nextT + 1) (depth - 1)
    else
      (nextT, depth)

def drainTerminals (terminals : List TerminalEvent) (startOffset : Nat) (st : OwnerState) :
    OwnerState :=
  let (nextT, depth) := drainLoop (terminals.drop st.nextTerminal) startOffset st.nextTerminal st.lexicalDepth
  { nextTerminal := nextT, lexicalDepth := depth }

/-- Exact model of `GroupOwners::outside_lexical_symbol`. -/
def outsideLexicalSymbol (terminals : List TerminalEvent) (st : OwnerState) (lex : Lexeme) :
    OwnerState × Option Nat :=
  let st1 := drainTerminals terminals lex.startOffset st
  match lex.kind with
  | .stringStart | .interpolationStart =>
    ({ st1 with lexicalDepth := st1.lexicalDepth + 1 }, none)
  | .stringEnd | .interpolationEnd =>
    ({ st1 with lexicalDepth := st1.lexicalDepth - 1 }, none)
  | .symbol sym =>
    if st1.lexicalDepth == 0 then (st1, some sym) else (st1, none)
  | .other =>
    (st1, none)

/-- Reference active owner-stack drain (matching `LexicalRecoveryIndex::new` validation). -/
def drainStack : List TerminalEvent → Nat → List OwnerKind → Option (List OwnerKind)
  | [], _, stack => some stack
  | ev :: rest, startOffset, stack =>
    if ev.offset ≤ startOffset then
      match stack with
      | top :: tail =>
        if top == ev.kind then drainStack rest startOffset tail else none
      | [] => none
    else
      some stack

/-- Reference active owner-stack step for one lexeme. -/
def stepStack (terminals : List TerminalEvent) (nextT : Nat) (stack : List OwnerKind) (lex : Lexeme) :
    Option (List OwnerKind × List OwnerKind) :=
  match drainStack (terminals.drop nextT) lex.startOffset stack with
  | none => none
  | some postDrain =>
    match lex.kind with
    | .stringStart => some (postDrain, .string :: postDrain)
    | .interpolationStart => some (postDrain, .interpolation :: postDrain)
    | .stringEnd =>
      match postDrain with
      | .string :: tail => some (postDrain, tail)
      | _ => none
    | .interpolationEnd =>
      match postDrain with
      | .interpolation :: tail => some (postDrain, tail)
      | _ => none
    | .symbol _ | .other => some (postDrain, postDrain)

/-! ### G1: `drainLoop` depth preservation, no-underflow, and terminal-index bounds -/

theorem drainLoop_spec (rem : List TerminalEvent) (startOffset : Nat) :
    ∀ (nextT depth : Nat) (stack postDrain : List OwnerKind),
      depth = stack.length →
      drainStack rem startOffset stack = some postDrain →
      let (nextT', depth') := drainLoop rem startOffset nextT depth
      depth' = postDrain.length ∧
      nextT ≤ nextT' ∧
      nextT' ≤ nextT + rem.length ∧
      (nextT' - nextT) + postDrain.length = stack.length := by
  induction rem with
  | nil =>
    intro nextT depth stack postDrain hdepth hdrain
    simp only [drainStack, Option.some.injEq] at hdrain
    subst hdrain
    simp only [drainLoop, List.length_nil, Nat.add_zero]
    omega
  | cons ev rest ih =>
    intro nextT depth stack postDrain hdepth hdrain
    unfold drainStack at hdrain
    unfold drainLoop
    by_cases hle : ev.offset ≤ startOffset
    · simp only [hle, ↓reduceIte] at hdrain ⊢
      cases stack with
      | nil => simp at hdrain
      | cons top tail =>
        by_cases hkind : top = ev.kind
        · subst hkind
          simp only [beq_self_eq_true, ↓reduceIte] at hdrain
          have hdepth' : depth - 1 = tail.length := by
            simp only [List.length_cons] at hdepth
            omega
          have hrec := ih (nextT + 1) (depth - 1) tail postDrain hdepth' hdrain
          rcases hrec with ⟨hd, hge, hub, hsum⟩
          refine ⟨hd, ?_, ?_, ?_⟩
          · omega
          · simp only [List.length_cons]; omega
          · simp only [List.length_cons]; omega
        · have hne : (top == ev.kind) = false := beq_false_of_ne hkind
          simp [hne] at hdrain
    · simp only [hle, ↓reduceIte, Option.some.injEq] at hdrain ⊢
      subst hdrain
      refine ⟨hdepth, Nat.le_refl _, ?_, ?_⟩
      · omega
      · omega

theorem drainTerminals_spec (terminals : List TerminalEvent) (startOffset : Nat)
    (st : OwnerState) (stack postDrain : List OwnerKind)
    (hbound : st.nextTerminal ≤ terminals.length)
    (hdepth : st.lexicalDepth = stack.length)
    (hdrain : drainStack (terminals.drop st.nextTerminal) startOffset stack = some postDrain) :
    let st1 := drainTerminals terminals startOffset st
    st1.lexicalDepth = postDrain.length ∧
    st.nextTerminal ≤ st1.nextTerminal ∧
    st1.nextTerminal ≤ terminals.length := by
  have h := drainLoop_spec (terminals.drop st.nextTerminal) startOffset st.nextTerminal st.lexicalDepth stack postDrain hdepth hdrain
  simp only [drainTerminals]
  rcases h with ⟨hd, hge, hub, _⟩
  refine ⟨hd, hge, ?_⟩
  have hlen : (terminals.drop st.nextTerminal).length = terminals.length - st.nextTerminal :=
    List.length_drop st.nextTerminal terminals
  omega

/-! ### G2 & G3: Single-step invariant preservation & top-level delimiter isolation -/

theorem outsideLexicalSymbol_preserves_invariant (terminals : List TerminalEvent)
    (st : OwnerState) (lex : Lexeme) (stack postDrain nextStack : List OwnerKind)
    (hbound : st.nextTerminal ≤ terminals.length)
    (hdepth : st.lexicalDepth = stack.length)
    (hstep : stepStack terminals st.nextTerminal stack lex = some (postDrain, nextStack)) :
    let (st', _) := outsideLexicalSymbol terminals st lex
    st'.lexicalDepth = nextStack.length ∧
    st.nextTerminal ≤ st'.nextTerminal ∧
    st'.nextTerminal ≤ terminals.length := by
  unfold stepStack at hstep
  cases hdrain : drainStack (terminals.drop st.nextTerminal) lex.startOffset stack with
  | none => simp [hdrain] at hstep
  | some pd =>
    simp only [hdrain] at hstep
    have hdt := drainTerminals_spec terminals lex.startOffset st stack pd hbound hdepth hdrain
    rcases hdt with ⟨hpd_depth, hge, hub⟩
    unfold outsideLexicalSymbol
    cases hkind : lex.kind with
    | stringStart =>
      simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
      rcases hstep with ⟨rfl, rfl⟩
      exact ⟨by simp [hpd_depth], hge, hub⟩
    | interpolationStart =>
      simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
      rcases hstep with ⟨rfl, rfl⟩
      exact ⟨by simp [hpd_depth], hge, hub⟩
    | stringEnd =>
      simp only [hkind] at hstep
      cases pd with
      | nil => simp at hstep
      | cons top tail =>
        cases top with
        | interpolation => simp at hstep
        | string =>
          simp only [Option.some.injEq, Prod.mk.injEq] at hstep
          rcases hstep with ⟨rfl, rfl⟩
          exact ⟨by simp only [List.length_cons] at hpd_depth; dsimp only; omega, hge, hub⟩
    | interpolationEnd =>
      simp only [hkind] at hstep
      cases pd with
      | nil => simp at hstep
      | cons top tail =>
        cases top with
        | string => simp at hstep
        | interpolation =>
          simp only [Option.some.injEq, Prod.mk.injEq] at hstep
          rcases hstep with ⟨rfl, rfl⟩
          exact ⟨by simp only [List.length_cons] at hpd_depth; dsimp only; omega, hge, hub⟩
    | symbol sym =>
      simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
      rcases hstep with ⟨rfl, rfl⟩
      by_cases hz : (drainTerminals terminals lex.startOffset st).lexicalDepth == 0
      · simp only [hz, ↓reduceIte]
        exact ⟨hpd_depth, hge, hub⟩
      · simp only [hz, Bool.false_eq_true, ↓reduceIte]
        exact ⟨hpd_depth, hge, hub⟩
    | other =>
      simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
      rcases hstep with ⟨rfl, rfl⟩
      exact ⟨hpd_depth, hge, hub⟩

/-- G3: `outsideLexicalSymbol` emits `some sym` iff the lexeme is `symbol sym` AND
    the post-drain active lexical owner stack is empty (strings/interpolations never leak). -/
theorem outsideLexicalSymbol_some_iff_top_level (terminals : List TerminalEvent)
    (st : OwnerState) (lex : Lexeme) (stack postDrain nextStack : List OwnerKind) (sym : Nat)
    (hbound : st.nextTerminal ≤ terminals.length)
    (hdepth : st.lexicalDepth = stack.length)
    (hstep : stepStack terminals st.nextTerminal stack lex = some (postDrain, nextStack)) :
    (outsideLexicalSymbol terminals st lex).2 = some sym ↔
      lex.kind = .symbol sym ∧ postDrain = [] := by
  unfold stepStack at hstep
  cases hdrain : drainStack (terminals.drop st.nextTerminal) lex.startOffset stack with
  | none => simp [hdrain] at hstep
  | some pd =>
    simp only [hdrain] at hstep
    have hdt := drainTerminals_spec terminals lex.startOffset st stack pd hbound hdepth hdrain
    rcases hdt with ⟨hpd_depth, _, _⟩
    unfold outsideLexicalSymbol
    cases hkind : lex.kind with
    | stringStart => simp [hkind]
    | interpolationStart => simp [hkind]
    | stringEnd => simp [hkind]
    | interpolationEnd => simp [hkind]
    | other => simp [hkind]
    | symbol s =>
      simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
      rcases hstep with ⟨rfl, rfl⟩
      cases pd with
      | nil =>
        simp only [List.length_nil] at hpd_depth
        simp [hpd_depth, eq_comm]
      | cons top tail =>
        simp only [List.length_cons] at hpd_depth
        have hne : ((drainTerminals terminals lex.startOffset st).lexicalDepth == 0) = false := by
          simp [hpd_depth]
        simp [hne]

/-! ### G4: Arbitrary-length stream induction -/

def runOwners (terminals : List TerminalEvent) (st : OwnerState) : List Lexeme → OwnerState
  | [] => st
  | lex :: rest => runOwners terminals (outsideLexicalSymbol terminals st lex).1 rest

def ValidStream (terminals : List TerminalEvent) :
    Nat → List OwnerKind → List Lexeme → Prop
  | _, _, [] => True
  | nextT, stack, lex :: rest =>
    ∃ postDrain nextStack,
      stepStack terminals nextT stack lex = some (postDrain, nextStack) ∧
      ValidStream terminals
        (drainTerminals terminals lex.startOffset { nextTerminal := nextT, lexicalDepth := stack.length }).nextTerminal
        nextStack rest

theorem runOwners_preserves_bounds (terminals : List TerminalEvent) (lexemes : List Lexeme) :
    ∀ (st : OwnerState) (stack : List OwnerKind),
      st.nextTerminal ≤ terminals.length →
      st.lexicalDepth = stack.length →
      ValidStream terminals st.nextTerminal stack lexemes →
      let st' := runOwners terminals st lexemes
      st.nextTerminal ≤ st'.nextTerminal ∧
      st'.nextTerminal ≤ terminals.length := by
  induction lexemes with
  | nil =>
    intro st stack hbound _ _
    exact ⟨Nat.le_refl _, hbound⟩
  | cons lex rest ih =>
    intro st stack hbound hdepth hvalid
    rcases hvalid with ⟨postDrain, nextStack, hstep, hrest⟩
    have hinv := outsideLexicalSymbol_preserves_invariant terminals st lex stack postDrain nextStack hbound hdepth hstep
    rcases hinv with ⟨hdepth', hge, hub⟩
    have hst_eq : st = { nextTerminal := st.nextTerminal, lexicalDepth := stack.length } := by
      cases st
      simp only at hdepth
      simp [hdepth]
    have hnext_eq :
        (outsideLexicalSymbol terminals st lex).1.nextTerminal =
        (drainTerminals terminals lex.startOffset { nextTerminal := st.nextTerminal, lexicalDepth := stack.length }).nextTerminal := by
      rw [← hst_eq]
      unfold outsideLexicalSymbol
      cases hkind : lex.kind with
      | stringStart => rfl
      | interpolationStart => rfl
      | stringEnd => rfl
      | interpolationEnd => rfl
      | other => rfl
      | symbol s =>
        by_cases hz : (drainTerminals terminals lex.startOffset st).lexicalDepth == 0 <;> simp [hz]
    rw [← hnext_eq] at hrest
    have hrec := ih (outsideLexicalSymbol terminals st lex).1 nextStack hub hdepth' hrest
    exact ⟨Nat.le_trans hge hrec.1, hrec.2⟩

end F6GroupOwners
