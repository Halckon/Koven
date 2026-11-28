/-
  SPEC-0510 (F6 Slice 11): Machine-checked unbounded model proofs for
  `owner_events::verify_terminal_owner_events` lexical owner-stack LIFO
  soundness, exact EOF balance, rejection completeness, and offset boundedness
  (`crates/lang-frontend/src/parser/engine/recovery/owner_events.rs`).

  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, unbounded)` -- NOT full-input production G4.
  Paired with Kani direct-Rust harnesses in `owner_events_kani.rs`.
-/

namespace F6OwnerEvents

inductive OwnerKind where
  | string
  | interpolation
  deriving DecidableEq, Repr

structure ActiveOwner where
  kind : OwnerKind
  opener : Nat
  deriving DecidableEq, Repr

structure TerminalEvent where
  offset : Nat
  kind : OwnerKind
  opener : Nat
  deriving DecidableEq, Repr

inductive LexemeKind where
  | stringStart
  | interpolationStart
  | stringEnd
  | interpolationEnd
  | terminalEscape
  | eof
  | other
  deriving DecidableEq, Repr

structure Lexeme where
  startOffset : Nat
  endOffset : Nat
  kind : LexemeKind
  deriving DecidableEq, Repr

/-- Exact model of the `while terminal_owner_events.get(next_terminal_event)...` loop. -/
def drainDueEvents : List TerminalEvent → Nat → List ActiveOwner → Nat → List TerminalEvent →
    Option (List ActiveOwner × Nat × List TerminalEvent)
  | [], _, owners, nextT, verified => some (owners, nextT, verified)
  | ev :: rest, startOffset, owners, nextT, verified =>
    if ev.offset ≤ startOffset then
      match owners with
      | top :: tail =>
        if top.kind == ev.kind && top.opener == ev.opener then
          drainDueEvents rest startOffset tail (nextT + 1) (verified ++ [ev])
        else
          none
      | [] => none
    else
      some (owners, nextT, verified)

/-- Exact model of the EOF cascade `while let Some((kind, opener)) = active_owners.pop()` loop. -/
def drainEofOwners (sourceLen : Nat) : List ActiveOwner → List TerminalEvent → List TerminalEvent
  | [], verified => verified
  | top :: tail, verified =>
    drainEofOwners sourceLen tail
      (verified ++ [{ offset := sourceLen, kind := top.kind, opener := top.opener }])

/-- Exact model of one iteration of `for lexeme in lexemes`. -/
def verifyStep (inputEvents : List TerminalEvent) (terminalErrorAtEof : Bool) (sourceLen : Nat)
    (owners : List ActiveOwner) (nextT : Nat) (verified : List TerminalEvent) (lex : Lexeme) :
    Option (List ActiveOwner × Nat × List TerminalEvent) :=
  match drainDueEvents (inputEvents.drop nextT) lex.startOffset owners nextT verified with
  | none => none
  | some (owners1, nextT1, verified1) =>
    match lex.kind with
    | .eof =>
      if owners1.isEmpty then
        some ([], nextT1, verified1)
      else if terminalErrorAtEof then
        some ([], nextT1, drainEofOwners sourceLen owners1 verified1)
      else
        none
    | .stringStart =>
      some ({ kind := .string, opener := lex.startOffset } :: owners1, nextT1, verified1)
    | .interpolationStart =>
      some ({ kind := .interpolation, opener := lex.startOffset } :: owners1, nextT1, verified1)
    | .stringEnd =>
      match owners1 with
      | top :: tail =>
        if top.kind == .string then some (tail, nextT1, verified1) else none
      | [] => none
    | .interpolationEnd =>
      match owners1 with
      | top :: tail =>
        if top.kind == .interpolation then some (tail, nextT1, verified1) else none
      | [] => none
    | .terminalEscape =>
      match owners1 with
      | top :: tail =>
        if top.kind == .string then
          some (tail, nextT1,
            verified1 ++ [{ offset := lex.endOffset, kind := .string, opener := top.opener }])
        else
          none
      | [] => none
    | .other =>
      some (owners1, nextT1, verified1)

/-- Full lexeme-stream fold. -/
def verifyLoop (inputEvents : List TerminalEvent) (terminalErrorAtEof : Bool) (sourceLen : Nat) :
    List ActiveOwner → Nat → List TerminalEvent → List Lexeme →
    Option (List ActiveOwner × Nat × List TerminalEvent)
  | owners, nextT, verified, [] => some (owners, nextT, verified)
  | owners, nextT, verified, lex :: rest =>
    match verifyStep inputEvents terminalErrorAtEof sourceLen owners nextT verified lex with
    | none => none
    | some (owners1, nextT1, verified1) =>
      verifyLoop inputEvents terminalErrorAtEof sourceLen owners1 nextT1 verified1 rest

/-- Top-level model of `owner_events::verify_terminal_owner_events`. -/
def verifyTerminalOwnerEvents (lexemes : List Lexeme) (inputEvents : List TerminalEvent)
    (terminalErrorAtEof : Bool) (sourceLen : Nat) : Option (List TerminalEvent) :=
  match verifyLoop inputEvents terminalErrorAtEof sourceLen [] 0 [] lexemes with
  | none => none
  | some (_, nextT, verified) =>
    if nextT == inputEvents.length then some verified else none

def openDelta : LexemeKind → Nat
  | .stringStart | .interpolationStart => 1
  | _ => 0

def closeDelta : LexemeKind → Nat
  | .stringEnd | .interpolationEnd => 1
  | _ => 0

def countOpens : List Lexeme → Nat
  | [] => 0
  | lex :: rest => openDelta lex.kind + countOpens rest

def countExplicitCloses : List Lexeme → Nat
  | [] => 0
  | lex :: rest => closeDelta lex.kind + countExplicitCloses rest

/-! ### O1: `drainDueEvents` LIFO balance & index bounds -/

theorem drainDueEvents_balance_and_bounds (rem : List TerminalEvent) (startOffset : Nat) :
    ∀ (owners owners' : List ActiveOwner) (nextT nextT' : Nat)
      (verified verified' : List TerminalEvent),
      drainDueEvents rem startOffset owners nextT verified = some (owners', nextT', verified') →
      owners.length + verified.length = owners'.length + verified'.length ∧
      nextT ≤ nextT' ∧
      nextT' ≤ nextT + rem.length ∧
      verified.length ≤ verified'.length ∧
      nextT' - nextT = verified'.length - verified.length := by
  induction rem with
  | nil =>
    intro owners owners' nextT nextT' verified verified' h
    simp only [drainDueEvents, Option.some.injEq, Prod.mk.injEq] at h
    rcases h with ⟨rfl, rfl, rfl⟩
    simp
  | cons ev rest ih =>
    intro owners owners' nextT nextT' verified verified' h
    unfold drainDueEvents at h
    by_cases hle : ev.offset ≤ startOffset
    · simp only [hle, ↓reduceIte] at h
      cases owners with
      | nil => simp at h
      | cons top tail =>
        by_cases hmatch : (top.kind == ev.kind && top.opener == ev.opener) = true
        · simp only [hmatch, ↓reduceIte] at h
          have hrec := ih tail owners' (nextT + 1) nextT' (verified ++ [ev]) verified' h
          rcases hrec with ⟨hbal, hge, hub, hvle, hdiff⟩
          simp only [List.length_append, List.length_cons, List.length_nil] at hbal hvle hdiff
          simp only [List.length_cons]
          exact ⟨by omega, by omega, by omega, by omega, by omega⟩
        · simp only [hmatch, Bool.false_eq_true, ↓reduceIte] at h
          contradiction
    · simp only [hle, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at h
      rcases h with ⟨rfl, rfl, rfl⟩
      simp

/-! ### O2: `drainEofOwners` exact event count -/

theorem drainEofOwners_length (sourceLen : Nat) (owners : List ActiveOwner) :
    ∀ (verified : List TerminalEvent),
      (drainEofOwners sourceLen owners verified).length = verified.length + owners.length := by
  induction owners with
  | nil =>
    intro verified
    simp [drainEofOwners]
  | cons top tail ih =>
    intro verified
    simp only [drainEofOwners, ih, List.length_append, List.length_cons, List.length_nil]
    omega

/-! ### O3: Single-step balance & monotonicity invariant -/

theorem verifyStep_balance_invariant (inputEvents : List TerminalEvent)
    (terminalErrorAtEof : Bool) (sourceLen : Nat)
    (owners owners' : List ActiveOwner) (nextT nextT' : Nat)
    (verified verified' : List TerminalEvent) (lex : Lexeme)
    (hbound : nextT ≤ inputEvents.length)
    (hstep : verifyStep inputEvents terminalErrorAtEof sourceLen owners nextT verified lex =
      some (owners', nextT', verified')) :
    owners.length + verified.length + openDelta lex.kind =
      owners'.length + verified'.length + closeDelta lex.kind ∧
    nextT ≤ nextT' ∧
    nextT' ≤ inputEvents.length ∧
    verified.length ≤ verified'.length := by
  unfold verifyStep at hstep
  cases hdrain : drainDueEvents (inputEvents.drop nextT) lex.startOffset owners nextT verified with
  | none => simp [hdrain] at hstep
  | some res =>
    rcases res with ⟨owners1, nextT1, verified1⟩
    simp only [hdrain] at hstep
    have hd := drainDueEvents_balance_and_bounds (inputEvents.drop nextT) lex.startOffset
      owners owners1 nextT nextT1 verified verified1 hdrain
    rcases hd with ⟨hbal1, hge1, hub1, hvle1, _⟩
    have hdrop : (inputEvents.drop nextT).length = inputEvents.length - nextT :=
      List.length_drop nextT inputEvents
    have hub_in : nextT1 ≤ inputEvents.length := by omega
    cases hkind : lex.kind with
    | eof =>
      simp only [hkind] at hstep
      by_cases hempty : owners1.isEmpty
      · simp only [hempty, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
        rcases hstep with ⟨rfl, rfl, rfl⟩
        have hlen0 : owners1.length = 0 := by
          cases owners1 with
          | nil => rfl
          | cons _ _ => simp at hempty
        simp only [openDelta, closeDelta, List.length_nil]
        omega
      · simp only [hempty, Bool.false_eq_true, ↓reduceIte] at hstep
        by_cases herr : terminalErrorAtEof
        · simp only [herr, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
          rcases hstep with ⟨rfl, rfl, rfl⟩
          have heof := drainEofOwners_length sourceLen owners1 verified1
          simp only [openDelta, closeDelta, List.length_nil]
          omega
        · simp only [herr, Bool.false_eq_true, ↓reduceIte] at hstep
          contradiction
    | stringStart =>
      simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
      rcases hstep with ⟨rfl, rfl, rfl⟩
      simp only [openDelta, closeDelta, List.length_cons]
      omega
    | interpolationStart =>
      simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
      rcases hstep with ⟨rfl, rfl, rfl⟩
      simp only [openDelta, closeDelta, List.length_cons]
      omega
    | stringEnd =>
      simp only [hkind] at hstep
      cases owners1 with
      | nil => simp at hstep
      | cons top tail =>
        by_cases hk : (top.kind == .string) = true
        · simp only [hk, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
          rcases hstep with ⟨rfl, rfl, rfl⟩
          simp only [openDelta, closeDelta, List.length_cons] at hbal1 ⊢
          omega
        · simp only [hk, Bool.false_eq_true, ↓reduceIte] at hstep
          contradiction
    | interpolationEnd =>
      simp only [hkind] at hstep
      cases owners1 with
      | nil => simp at hstep
      | cons top tail =>
        by_cases hk : (top.kind == .interpolation) = true
        · simp only [hk, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
          rcases hstep with ⟨rfl, rfl, rfl⟩
          simp only [openDelta, closeDelta, List.length_cons] at hbal1 ⊢
          omega
        · simp only [hk, Bool.false_eq_true, ↓reduceIte] at hstep
          contradiction
    | terminalEscape =>
      simp only [hkind] at hstep
      cases owners1 with
      | nil => simp at hstep
      | cons top tail =>
        by_cases hk : (top.kind == .string) = true
        · simp only [hk, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
          rcases hstep with ⟨rfl, rfl, rfl⟩
          simp only [openDelta, closeDelta, List.length_cons, List.length_append, List.length_nil] at hbal1 ⊢
          exact ⟨by omega, by omega, by omega, by omega⟩
        · simp only [hk, Bool.false_eq_true, ↓reduceIte] at hstep
          contradiction
    | other =>
      simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
      rcases hstep with ⟨rfl, rfl, rfl⟩
      simp only [openDelta, closeDelta]
      omega

/-! ### O4: Arbitrary-length stream global balance & EOF closure -/

theorem verifyLoop_global_balance (inputEvents : List TerminalEvent)
    (terminalErrorAtEof : Bool) (sourceLen : Nat) (lexemes : List Lexeme) :
    ∀ (owners owners' : List ActiveOwner) (nextT nextT' : Nat)
      (verified verified' : List TerminalEvent),
      nextT ≤ inputEvents.length →
      verifyLoop inputEvents terminalErrorAtEof sourceLen owners nextT verified lexemes =
        some (owners', nextT', verified') →
      owners.length + verified.length + countOpens lexemes =
        owners'.length + verified'.length + countExplicitCloses lexemes ∧
      nextT ≤ nextT' ∧
      nextT' ≤ inputEvents.length := by
  induction lexemes with
  | nil =>
    intro owners owners' nextT nextT' verified verified' hbound hloop
    simp only [verifyLoop, Option.some.injEq, Prod.mk.injEq] at hloop
    rcases hloop with ⟨rfl, rfl, rfl⟩
    simp [countOpens, countExplicitCloses, hbound]
  | cons lex rest ih =>
    intro owners owners' nextT nextT' verified verified' hbound hloop
    unfold verifyLoop at hloop
    cases hstep : verifyStep inputEvents terminalErrorAtEof sourceLen owners nextT verified lex with
    | none => simp [hstep] at hloop
    | some res =>
      rcases res with ⟨owners1, nextT1, verified1⟩
      simp only [hstep] at hloop
      have hs := verifyStep_balance_invariant inputEvents terminalErrorAtEof sourceLen
        owners owners1 nextT nextT1 verified verified1 lex hbound hstep
      rcases hs with ⟨hbal1, hge1, hub1, _⟩
      have hrec := ih owners1 owners' nextT1 nextT' verified1 verified' hub1 hloop
      rcases hrec with ⟨hbal2, hge2, hub2⟩
      simp only [countOpens, countExplicitCloses]
      omega

theorem verifyLoop_append (inputEvents : List TerminalEvent)
    (terminalErrorAtEof : Bool) (sourceLen : Nat) (xs ys : List Lexeme) :
    ∀ (owners : List ActiveOwner) (nextT : Nat) (verified : List TerminalEvent),
      verifyLoop inputEvents terminalErrorAtEof sourceLen owners nextT verified (xs ++ ys) =
      match verifyLoop inputEvents terminalErrorAtEof sourceLen owners nextT verified xs with
      | none => none
      | some (owners1, nextT1, verified1) =>
        verifyLoop inputEvents terminalErrorAtEof sourceLen owners1 nextT1 verified1 ys := by
  induction xs with
  | nil =>
    intro owners nextT verified
    rfl
  | cons x rest ih =>
    intro owners nextT verified
    simp only [List.cons_append, verifyLoop]
    cases hstep : verifyStep inputEvents terminalErrorAtEof sourceLen owners nextT verified x with
    | none => rfl
    | some res =>
      rcases res with ⟨owners1, nextT1, verified1⟩
      exact ih owners1 nextT1 verified1

theorem verifyTerminalOwnerEvents_eof_closed_balance (prefixLex : List Lexeme) (eofLex : Lexeme)
    (inputEvents verified : List TerminalEvent) (terminalErrorAtEof : Bool) (sourceLen : Nat)
    (heof : eofLex.kind = .eof)
    (hok : verifyTerminalOwnerEvents (prefixLex ++ [eofLex]) inputEvents terminalErrorAtEof sourceLen =
      some verified) :
    countOpens (prefixLex ++ [eofLex]) =
      countExplicitCloses (prefixLex ++ [eofLex]) + verified.length := by
  unfold verifyTerminalOwnerEvents at hok
  cases hloop : verifyLoop inputEvents terminalErrorAtEof sourceLen [] 0 [] (prefixLex ++ [eofLex]) with
  | none => simp [hloop] at hok
  | some res =>
    rcases res with ⟨owners', nextT', verified'⟩
    simp only [hloop] at hok
    by_cases hfull : (nextT' == inputEvents.length) = true
    · simp only [hfull, ↓reduceIte, Option.some.injEq] at hok
      subst hok
      have hbal := verifyLoop_global_balance inputEvents terminalErrorAtEof sourceLen
        (prefixLex ++ [eofLex]) [] owners' 0 nextT' [] verified' (Nat.zero_le _) hloop
      rw [verifyLoop_append] at hloop
      cases hp : verifyLoop inputEvents terminalErrorAtEof sourceLen [] 0 [] prefixLex with
      | none => simp [hp] at hloop
      | some pRes =>
        rcases pRes with ⟨ownersP, nextTP, verifiedP⟩
        simp only [hp, verifyLoop] at hloop
        cases hs : verifyStep inputEvents terminalErrorAtEof sourceLen ownersP nextTP verifiedP eofLex with
        | none => simp [hs] at hloop
        | some sRes =>
          rcases sRes with ⟨ownersS, nextTS, verifiedS⟩
          simp only [hs, Option.some.injEq, Prod.mk.injEq] at hloop
          rcases hloop with ⟨rfl, rfl, rfl⟩
          unfold verifyStep at hs
          cases hd : drainDueEvents (inputEvents.drop nextTP) eofLex.startOffset ownersP nextTP verifiedP with
          | none => simp [hd] at hs
          | some dRes =>
            rcases dRes with ⟨owners1, nextT1, verified1⟩
            simp only [hd, heof] at hs
            by_cases hempty : owners1.isEmpty
            · simp only [hempty, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hs
              rcases hs with ⟨rfl, _, _⟩
              simp only [List.length_nil] at hbal
              omega
            · simp only [hempty, Bool.false_eq_true, ↓reduceIte] at hs
              by_cases herr : terminalErrorAtEof
              · simp only [herr, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hs
                rcases hs with ⟨rfl, _, _⟩
                simp only [List.length_nil] at hbal
                omega
              · simp only [herr, Bool.false_eq_true, ↓reduceIte] at hs
                contradiction
    · simp only [hfull, Bool.false_eq_true, ↓reduceIte] at hok
      contradiction

/-! ### O5: Rejection completeness (unclosed EOF without flag & unconsumed input events) -/

theorem verifyStep_rejects_unclosed_eof_without_flag (inputEvents : List TerminalEvent)
    (sourceLen : Nat) (owners owners1 : List ActiveOwner) (nextT nextT1 : Nat)
    (verified verified1 : List TerminalEvent) (lex : Lexeme)
    (heof : lex.kind = .eof)
    (hdrain : drainDueEvents (inputEvents.drop nextT) lex.startOffset owners nextT verified =
      some (owners1, nextT1, verified1))
    (hne : owners1 ≠ []) :
    verifyStep inputEvents false sourceLen owners nextT verified lex = none := by
  unfold verifyStep
  rw [hdrain]
  simp only [heof]
  cases owners1 with
  | nil => exact absurd rfl hne
  | cons _ _ => simp [List.isEmpty]

theorem verifyTerminalOwnerEvents_rejects_unconsumed (lexemes : List Lexeme)
    (inputEvents : List TerminalEvent) (terminalErrorAtEof : Bool) (sourceLen : Nat)
    (owners' : List ActiveOwner) (nextT' : Nat) (verified' : List TerminalEvent)
    (hloop : verifyLoop inputEvents terminalErrorAtEof sourceLen [] 0 [] lexemes =
      some (owners', nextT', verified'))
    (hlt : nextT' < inputEvents.length) :
    verifyTerminalOwnerEvents lexemes inputEvents terminalErrorAtEof sourceLen = none := by
  unfold verifyTerminalOwnerEvents
  rw [hloop]
  have hne : (nextT' == inputEvents.length) = false := beq_false_of_ne (Nat.ne_of_lt hlt)
  simp [hne]

/-! ### O6: Verified event offset boundedness (`∀ ev ∈ verified, ev.offset ≤ sourceLen`) -/

def EventsBounded (sourceLen : Nat) (evs : List TerminalEvent) : Prop :=
  ∀ ev ∈ evs, ev.offset ≤ sourceLen

theorem drainDueEvents_preserves_bounded (sourceLen : Nat) (rem : List TerminalEvent)
    (startOffset : Nat) :
    ∀ (owners owners' : List ActiveOwner) (nextT nextT' : Nat)
      (verified verified' : List TerminalEvent),
      EventsBounded sourceLen rem →
      EventsBounded sourceLen verified →
      drainDueEvents rem startOffset owners nextT verified = some (owners', nextT', verified') →
      EventsBounded sourceLen verified' := by
  induction rem with
  | nil =>
    intro owners owners' nextT nextT' verified verified' _ hv h
    simp only [drainDueEvents, Option.some.injEq, Prod.mk.injEq] at h
    rcases h with ⟨rfl, rfl, rfl⟩
    exact hv
  | cons ev rest ih =>
    intro owners owners' nextT nextT' verified verified' hrem hv h
    unfold drainDueEvents at h
    by_cases hle : ev.offset ≤ startOffset
    · simp only [hle, ↓reduceIte] at h
      cases owners with
      | nil => simp at h
      | cons top tail =>
        by_cases hmatch : (top.kind == ev.kind && top.opener == ev.opener) = true
        · simp only [hmatch, ↓reduceIte] at h
          have hrest : EventsBounded sourceLen rest := fun x hx =>
            hrem x (List.mem_cons_of_mem ev hx)
          have hev : ev.offset ≤ sourceLen := hrem ev (List.mem_cons_self ev rest)
          have hv' : EventsBounded sourceLen (verified ++ [ev]) := by
            intro x hx
            rw [List.mem_append, List.mem_singleton] at hx
            rcases hx with hx | rfl
            · exact hv x hx
            · exact hev
          exact ih tail owners' (nextT + 1) nextT' (verified ++ [ev]) verified' hrest hv' h
        · simp only [hmatch, Bool.false_eq_true, ↓reduceIte] at h
          contradiction
    · simp only [hle, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at h
      rcases h with ⟨rfl, rfl, rfl⟩
      exact hv

theorem drainEofOwners_preserves_bounded (sourceLen : Nat) (owners : List ActiveOwner) :
    ∀ (verified : List TerminalEvent),
      EventsBounded sourceLen verified →
      EventsBounded sourceLen (drainEofOwners sourceLen owners verified) := by
  induction owners with
  | nil =>
    intro verified hv
    exact hv
  | cons top tail ih =>
    intro verified hv
    apply ih
    intro x hx
    rw [List.mem_append, List.mem_singleton] at hx
    rcases hx with hx | rfl
    · exact hv x hx
    · exact Nat.le_refl sourceLen

theorem verifyLoop_preserves_bounded (inputEvents : List TerminalEvent)
    (terminalErrorAtEof : Bool) (sourceLen : Nat) (lexemes : List Lexeme) :
    ∀ (owners owners' : List ActiveOwner) (nextT nextT' : Nat)
      (verified verified' : List TerminalEvent),
      EventsBounded sourceLen inputEvents →
      (∀ lex ∈ lexemes, lex.endOffset ≤ sourceLen) →
      EventsBounded sourceLen verified →
      verifyLoop inputEvents terminalErrorAtEof sourceLen owners nextT verified lexemes =
        some (owners', nextT', verified') →
      EventsBounded sourceLen verified' := by
  induction lexemes with
  | nil =>
    intro owners owners' nextT nextT' verified verified' _ _ hv hloop
    simp only [verifyLoop, Option.some.injEq, Prod.mk.injEq] at hloop
    rcases hloop with ⟨rfl, rfl, rfl⟩
    exact hv
  | cons lex rest ih =>
    intro owners owners' nextT nextT' verified verified' hin hlex hv hloop
    unfold verifyLoop at hloop
    cases hstep : verifyStep inputEvents terminalErrorAtEof sourceLen owners nextT verified lex with
    | none => simp [hstep] at hloop
    | some res =>
      rcases res with ⟨owners1, nextT1, verified1⟩
      simp only [hstep] at hloop
      have hdrop : EventsBounded sourceLen (inputEvents.drop nextT) := fun x hx =>
        hin x (List.mem_of_mem_drop hx)
      have hv1 : EventsBounded sourceLen verified1 := by
        unfold verifyStep at hstep
        cases hd : drainDueEvents (inputEvents.drop nextT) lex.startOffset owners nextT verified with
        | none => simp [hd] at hstep
        | some dRes =>
          rcases dRes with ⟨od, nd, vd⟩
          simp only [hd] at hstep
          have hvd := drainDueEvents_preserves_bounded sourceLen (inputEvents.drop nextT)
            lex.startOffset owners od nextT nd verified vd hdrop hv hd
          cases hkind : lex.kind with
          | eof =>
            simp only [hkind] at hstep
            by_cases hempty : od.isEmpty
            · simp only [hempty, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
              rcases hstep with ⟨rfl, rfl, rfl⟩
              exact hvd
            · simp only [hempty, Bool.false_eq_true, ↓reduceIte] at hstep
              by_cases herr : terminalErrorAtEof
              · simp only [herr, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
                rcases hstep with ⟨rfl, rfl, rfl⟩
                exact drainEofOwners_preserves_bounded sourceLen od vd hvd
              · simp only [herr, Bool.false_eq_true, ↓reduceIte] at hstep
                contradiction
          | stringStart =>
            simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
            rcases hstep with ⟨rfl, rfl, rfl⟩
            exact hvd
          | interpolationStart =>
            simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
            rcases hstep with ⟨rfl, rfl, rfl⟩
            exact hvd
          | stringEnd =>
            simp only [hkind] at hstep
            cases od with
            | nil => simp at hstep
            | cons top tail =>
              by_cases hk : (top.kind == .string) = true
              · simp only [hk, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
                rcases hstep with ⟨rfl, rfl, rfl⟩
                exact hvd
              · simp only [hk, Bool.false_eq_true, ↓reduceIte] at hstep
                contradiction
          | interpolationEnd =>
            simp only [hkind] at hstep
            cases od with
            | nil => simp at hstep
            | cons top tail =>
              by_cases hk : (top.kind == .interpolation) = true
              · simp only [hk, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
                rcases hstep with ⟨rfl, rfl, rfl⟩
                exact hvd
              · simp only [hk, Bool.false_eq_true, ↓reduceIte] at hstep
                contradiction
          | terminalEscape =>
            simp only [hkind] at hstep
            cases od with
            | nil => simp at hstep
            | cons top tail =>
              by_cases hk : (top.kind == .string) = true
              · simp only [hk, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at hstep
                rcases hstep with ⟨rfl, rfl, rfl⟩
                intro x hx
                rw [List.mem_append, List.mem_singleton] at hx
                rcases hx with hx | rfl
                · exact hvd x hx
                · exact hlex lex (List.mem_cons_self lex rest)
              · simp only [hk, Bool.false_eq_true, ↓reduceIte] at hstep
                contradiction
          | other =>
            simp only [hkind, Option.some.injEq, Prod.mk.injEq] at hstep
            rcases hstep with ⟨rfl, rfl, rfl⟩
            exact hvd
      have hrest : ∀ l ∈ rest, l.endOffset ≤ sourceLen := fun l hl =>
        hlex l (List.mem_cons_of_mem lex hl)
      exact ih owners1 owners' nextT1 nextT' verified1 verified' hin hrest hv1 hloop

theorem verifyTerminalOwnerEvents_bounded (lexemes : List Lexeme)
    (inputEvents verified : List TerminalEvent) (terminalErrorAtEof : Bool) (sourceLen : Nat)
    (hin : EventsBounded sourceLen inputEvents)
    (hlex : ∀ lex ∈ lexemes, lex.endOffset ≤ sourceLen)
    (hok : verifyTerminalOwnerEvents lexemes inputEvents terminalErrorAtEof sourceLen =
      some verified) :
    EventsBounded sourceLen verified := by
  unfold verifyTerminalOwnerEvents at hok
  cases hloop : verifyLoop inputEvents terminalErrorAtEof sourceLen [] 0 [] lexemes with
  | none => simp [hloop] at hok
  | some res =>
    rcases res with ⟨owners', nextT', verified'⟩
    simp only [hloop] at hok
    by_cases hfull : (nextT' == inputEvents.length) = true
    · simp only [hfull, ↓reduceIte, Option.some.injEq] at hok
      subst hok
      exact verifyLoop_preserves_bounded inputEvents terminalErrorAtEof sourceLen lexemes
        [] owners' 0 nextT' [] verified' hin hlex (fun _ h => by cases h) hloop
    · simp only [hfull, Bool.false_eq_true, ↓reduceIte] at hok
      contradiction

/-! ### O7: Exact per-event `(kind, opener)` LIFO stack-top identity match & mismatch rejection -/

def toActiveOwner (ev : TerminalEvent) : ActiveOwner :=
  { kind := ev.kind, opener := ev.opener }

theorem drainDueEvents_exact_lifo_match (rem : List TerminalEvent) (startOffset : Nat) :
    ∀ (owners owners' : List ActiveOwner) (nextT nextT' : Nat)
      (verified verified' : List TerminalEvent),
      drainDueEvents rem startOffset owners nextT verified = some (owners', nextT', verified') →
      ∃ emitted : List TerminalEvent,
        verified' = verified ++ emitted ∧
        owners = emitted.map toActiveOwner ++ owners' := by
  induction rem with
  | nil =>
    intro owners owners' nextT nextT' verified verified' h
    simp only [drainDueEvents, Option.some.injEq, Prod.mk.injEq] at h
    rcases h with ⟨rfl, rfl, rfl⟩
    exact ⟨[], by simp, by simp⟩
  | cons ev rest ih =>
    intro owners owners' nextT nextT' verified verified' h
    unfold drainDueEvents at h
    by_cases hle : ev.offset ≤ startOffset
    · simp only [hle, ↓reduceIte] at h
      cases owners with
      | nil => simp at h
      | cons top tail =>
        by_cases hmatch : (top.kind == ev.kind && top.opener == ev.opener) = true
        · simp only [hmatch, ↓reduceIte] at h
          have htop : top = toActiveOwner ev := by
            rw [Bool.and_eq_true, beq_iff_eq, beq_iff_eq] at hmatch
            cases top
            simp only [toActiveOwner] at hmatch ⊢
            rcases hmatch with ⟨hk, ho⟩
            subst hk; subst ho; rfl
          rcases ih tail owners' (nextT + 1) nextT' (verified ++ [ev]) verified' h with
            ⟨emitted, hveq, hstack⟩
          refine ⟨ev :: emitted, ?_, ?_⟩
          · rw [hveq, List.append_assoc]; rfl
          · rw [htop, hstack]; rfl
        · simp only [hmatch, Bool.false_eq_true, ↓reduceIte] at h
          contradiction
    · simp only [hle, ↓reduceIte, Option.some.injEq, Prod.mk.injEq] at h
      rcases h with ⟨rfl, rfl, rfl⟩
      exact ⟨[], by simp, by simp⟩

theorem drainEofOwners_exact_lifo_match (sourceLen : Nat) (owners : List ActiveOwner) :
    ∀ (verified : List TerminalEvent),
      ∃ emitted : List TerminalEvent,
        drainEofOwners sourceLen owners verified = verified ++ emitted ∧
        owners = emitted.map toActiveOwner := by
  induction owners with
  | nil =>
    intro verified
    exact ⟨[], by simp [drainEofOwners], by simp⟩
  | cons top tail ih =>
    intro verified
    let ev : TerminalEvent := { offset := sourceLen, kind := top.kind, opener := top.opener }
    rcases ih (verified ++ [ev]) with ⟨emitted, hveq, hstack⟩
    refine ⟨ev :: emitted, ?_, ?_⟩
    · simp only [drainEofOwners]
      rw [hveq, List.append_assoc]
      rfl
    · simp only [List.map_cons, toActiveOwner, ev]
      rw [← hstack]

theorem drainDueEvents_rejects_mismatched_top (ev : TerminalEvent) (rest : List TerminalEvent)
    (startOffset : Nat) (top : ActiveOwner) (tail : List ActiveOwner) (nextT : Nat)
    (verified : List TerminalEvent)
    (hle : ev.offset ≤ startOffset)
    (hmismatch : top.kind ≠ ev.kind ∨ top.opener ≠ ev.opener) :
    drainDueEvents (ev :: rest) startOffset (top :: tail) nextT verified = none := by
  unfold drainDueEvents
  simp only [hle, ↓reduceIte]
  have hfalse : (top.kind == ev.kind && top.opener == ev.opener) = false := by
    rcases hmismatch with hk | ho
    · simp [beq_false_of_ne hk]
    · simp [beq_false_of_ne ho]
  simp [hfalse]

end F6OwnerEvents
