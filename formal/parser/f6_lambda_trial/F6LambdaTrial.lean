/-
  SPEC-0509 (F6 seventh slice): Machine-checked unbounded model proofs for
  `LambdaHeaderIndex` single-pass streaming DFA equivalence, bounds, and linear
  transition count (`crates/lang-frontend/src/parser/lambda_trial.rs`).

  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, unbounded)` -- NOT full-input production G4.
  Paired with Kani direct-Rust harnesses in `lambda_trial.rs`.
-/

namespace F6LambdaTrial

inductive Tok where
  | trivia
  | leftBrace
  | ident
  | comma
  | arrow
  | other
  deriving DecidableEq, Repr

inductive TrialState where
  | start
  | afterIdent
  | afterComma
  deriving DecidableEq, Repr

structure Active where
  opener : Nat
  params : List Nat
  state : TrialState
  deriving DecidableEq, Repr

inductive Progress where
  | cont (next : Active)
  | matched
  | failed
  deriving DecidableEq, Repr

def Active.advance (a : Active) (raw : Nat) (tok : Tok) : Progress :=
  match a.state, tok with
  | .start, .ident | .afterComma, .ident =>
    .cont { a with params := a.params ++ [raw], state := .afterIdent }
  | .start, .arrow | .afterIdent, .arrow =>
    .matched
  | .afterIdent, .comma =>
    .cont { a with state := .afterComma }
  | _, _ => .failed

/-- Independent per-opener recursive specification for `[ ident { "," ident } ] "->"`
    ignoring `trivia`, starting at index `i`. -/
def specFrom (L : List Tok) : Nat → TrialState → List Nat → Option (List Nat × Nat)
  | i, st, acc =>
    if h : i < L.length then
      match L[i] with
      | .trivia => specFrom L (i + 1) st acc
      | .ident =>
        match st with
        | .start | .afterComma => specFrom L (i + 1) .afterIdent (acc ++ [i])
        | .afterIdent => none
      | .comma =>
        match st with
        | .afterIdent => specFrom L (i + 1) .afterComma acc
        | .start | .afterComma => none
      | .arrow =>
        match st with
        | .start | .afterIdent => some (acc, i)
        | .afterComma => none
      | .leftBrace | .other => none
    else
      none
termination_by i _ _ => L.length - i

def specHeader (L : List Tok) (opener : Nat) : Option (List Nat × Nat) :=
  specFrom L (opener + 1) .start []

def updateOut (out : Nat → Option (List Nat × Nat)) (k : Nat) (v : Option (List Nat × Nat)) :
    Nat → Option (List Nat × Nat) :=
  fun j => if j == k then v else out j

/-- Exact functional model of `LambdaHeaderIndex::new` single-pass loop
    (returning both the indexed header map and total DFA transitions). -/
def runPass (L : List Tok) :
    Nat → Option Active → (Nat → Option (List Nat × Nat)) → Nat →
    (Nat → Option (List Nat × Nat)) × Nat
  | i, active, out, steps =>
    if h : i < L.length then
      let tok := L[i]
      if tok == .trivia then
        runPass L (i + 1) active out steps
      else
        let (active1, out1, steps1) :=
          match active with
          | none => (none, out, steps)
          | some a =>
            match a.advance i tok with
            | .cont a' => (some a', out, steps + 1)
            | .failed => (none, out, steps + 1)
            | .matched => (none, updateOut out a.opener (some (a.params, i)), steps + 1)
        let active2 :=
          if tok == .leftBrace then
            some { opener := i, params := [], state := .start }
          else
            active1
        runPass L (i + 1) active2 out1 steps1
    else
      (out, steps)
termination_by i _ _ _ => L.length - i

def singlePass (L : List Tok) : Nat → Option (List Nat × Nat) :=
  (runPass L 0 none (fun _ => none) 0).1

def singlePassSteps (L : List Tok) : Nat :=
  (runPass L 0 none (fun _ => none) 0).2

/-! ### L1: A new `{` always fails any active DFA -/

theorem advance_leftBrace_fails (a : Active) (raw : Nat) :
    a.advance raw .leftBrace = .failed := by
  cases h : a.state <;> simp [Active.advance, h]

/-! ### L2: Single-step DFA transition matches `specFrom` -/

theorem specFrom_step_non_trivia (L : List Tok) (i : Nat) (hlt : i < L.length)
    (a : Active) (hnt : L[i] ≠ .trivia) :
    match a.advance i L[i] with
    | .cont a' => specFrom L i a.state a.params = specFrom L (i + 1) a'.state a'.params
    | .matched => specFrom L i a.state a.params = some (a.params, i)
    | .failed => specFrom L i a.state a.params = none := by
  cases htok : L[i] with
  | trivia => exact absurd htok hnt
  | leftBrace => cases hst : a.state <;> simp [Active.advance, hst] <;> (rw [specFrom.eq_def]; simp [hlt, htok])
  | other => cases hst : a.state <;> simp [Active.advance, hst] <;> (rw [specFrom.eq_def]; simp [hlt, htok])
  | ident => cases hst : a.state <;> simp [Active.advance, hst] <;> (rw [specFrom.eq_def]; simp [hlt, htok, hst])
  | comma => cases hst : a.state <;> simp [Active.advance, hst] <;> (rw [specFrom.eq_def]; simp [hlt, htok, hst])
  | arrow => cases hst : a.state <;> simp [Active.advance, hst] <;> (rw [specFrom.eq_def]; simp [hlt, htok, hst])

/-! ### L3: Unbounded single-pass soundness & completeness (`∀ L, ∀ k < L.length`) -/

private def PassInv (L : List Tok) (i : Nat) (active : Option Active)
    (out : Nat → Option (List Nat × Nat)) : Prop :=
  (∀ a, active = some a → a.opener < i ∧ L[a.opener]? = some .leftBrace ∧
      out a.opener = none ∧ specHeader L a.opener = specFrom L i a.state a.params) ∧
  (∀ k, k < i → (∀ a, active = some a → k ≠ a.opener) →
      out k = if L[k]? = some .leftBrace then specHeader L k else none) ∧
  (∀ k, i ≤ k → out k = none)

private theorem runPass_preserves_inv (L : List Tok) :
    ∀ (i : Nat) (active : Option Active) (out : Nat → Option (List Nat × Nat)) (steps : Nat),
      i ≤ L.length →
      PassInv L i active out →
      ∀ k, k < L.length →
        (runPass L i active out steps).1 k =
          if L[k]? = some .leftBrace then specHeader L k else none := by
  intro i active out steps hle hinv
  unfold runPass
  by_cases hlt : i < L.length
  · simp only [hlt, ↓reduceDIte]
    cases htok : L[i] with
    | trivia =>
      simp only [beq_self_eq_true, ↓reduceIte]
      rcases hinv with ⟨hact, hdone, hfut⟩
      apply runPass_preserves_inv L (i + 1) active out steps (by omega)
      refine ⟨?_, ?_, ?_⟩
      · intro a ha
        rcases hact a ha with ⟨hop, hlb, hnone, hspec⟩
        have htriv_eq : specFrom L i a.state a.params = specFrom L (i + 1) a.state a.params := by
          rw [specFrom.eq_def]
          simp [hlt, htok]
        exact ⟨by omega, hlb, hnone, hspec.trans htriv_eq⟩
      · intro k hk hne
        by_cases hki : k < i
        · exact hdone k hki hne
        · have heq : k = i := by omega
          subst heq
          have hget : L[k]? = some .trivia := by simp [List.getElem?_eq_getElem hlt, htok]
          simp [hfut k (Nat.le_refl _), hget]
      · intro k hk
        exact hfut k (by omega)
    | leftBrace =>
      simp only [reduceCtorEq, beq_iff_eq, ↓reduceIte, beq_self_eq_true]
      cases active with
      | none =>
        rcases hinv with ⟨_, hdone, hfut⟩
        apply runPass_preserves_inv L (i + 1) (some { opener := i, params := [], state := .start })
          out steps (by omega)
        refine ⟨?_, ?_, ?_⟩
        · intro a ha
          cases ha
          exact ⟨by dsimp only; omega, by simp [List.getElem?_eq_getElem hlt, htok], hfut i (Nat.le_refl _), rfl⟩
        · intro k hk hne
          have hki : k < i := by
            have hne_i : k ≠ i := hne { opener := i, params := [], state := .start } rfl
            omega
          exact hdone k hki (fun _ hc => nomatch hc)
        · intro k hk
          exact hfut k (by omega)
      | some a =>
        simp only [advance_leftBrace_fails]
        rcases hinv with ⟨hact, hdone, hfut⟩
        rcases hact a rfl with ⟨_, hlb, hnone, hspec⟩
        apply runPass_preserves_inv L (i + 1) (some { opener := i, params := [], state := .start })
          out (steps + 1) (by omega)
        refine ⟨?_, ?_, ?_⟩
        · intro b hb
          cases hb
          exact ⟨by dsimp only; omega, by simp [List.getElem?_eq_getElem hlt, htok], hfut i (Nat.le_refl _), rfl⟩
        · intro k hk hne
          have hki : k < i := by
            have hne_i : k ≠ i := hne { opener := i, params := [], state := .start } rfl
            omega
          by_cases hka : k = a.opener
          · subst hka
            rw [hnone, hlb]
            simp only [↓reduceIte]
            rw [hspec]
            unfold specFrom
            simp [hlt, htok]
          · exact hdone k hki (fun b hb => by cases hb; exact hka)
        · intro k hk
          exact hfut k (by omega)
    | ident | comma | arrow | other =>
      simp only [reduceCtorEq, beq_iff_eq, ↓reduceIte]
      cases active with
      | none =>
        rcases hinv with ⟨_, hdone, hfut⟩
        apply runPass_preserves_inv L (i + 1) none out steps (by omega)
        refine ⟨(fun _ hc => nomatch hc), ?_, ?_⟩
        · intro k hk _
          by_cases hki : k < i
          · exact hdone k hki (fun _ hc => nomatch hc)
          · have heq : k = i := by omega
            subst heq
            have hget : L[k]? ≠ some .leftBrace := by
              simp [List.getElem?_eq_getElem hlt, htok]
            simp [hfut k (Nat.le_refl _), hget]
        · intro k hk
          exact hfut k (by omega)
      | some a =>
        rcases hinv with ⟨hact, hdone, hfut⟩
        rcases hact a rfl with ⟨hop, hlb, hnone, hspec⟩
        have hstep := specFrom_step_non_trivia L i hlt a (by simp [htok])
        simp only [htok] at hstep
        cases hadv : a.advance i (L[i]) with
        | cont a' =>
          simp only [htok] at hadv
          simp only [hadv] at hstep ⊢
          have hop_eq : a'.opener = a.opener := by
            cases hst : a.state <;> simp [Active.advance, hst] at hadv
            all_goals cases hadv; rfl
          apply runPass_preserves_inv L (i + 1) (some a') out (steps + 1) (by omega)
          refine ⟨?_, ?_, ?_⟩
          · intro b hb
            cases hb
            rw [hop_eq]
            exact ⟨by omega, hlb, hnone, hspec.trans hstep⟩
          · intro k hk hne
            by_cases hki : k < i
            · apply hdone k hki
              intro b hb
              cases hb
              rw [← hop_eq]
              exact hne a' rfl
            · have heq : k = i := by omega
              subst heq
              have hget : L[k]? ≠ some .leftBrace := by
                simp [List.getElem?_eq_getElem hlt, htok]
              simp [hfut k (Nat.le_refl _), hget]
          · intro k hk
            exact hfut k (by omega)
        | failed =>
          simp only [htok] at hadv
          simp only [hadv] at hstep ⊢
          apply runPass_preserves_inv L (i + 1) none out (steps + 1) (by omega)
          refine ⟨(fun _ hc => nomatch hc), ?_, ?_⟩
          · intro k hk _
            by_cases hki : k < i
            · by_cases hka : k = a.opener
              · subst hka
                rw [hnone, hlb]
                simp [hspec, hstep]
              · exact hdone k hki (fun b hb => by cases hb; exact hka)
            · have heq : k = i := by omega
              subst heq
              have hget : L[k]? ≠ some .leftBrace := by
                simp [List.getElem?_eq_getElem hlt, htok]
              simp [hfut k (Nat.le_refl _), hget]
          · intro k hk
            exact hfut k (by omega)
        | matched =>
          simp only [htok] at hadv
          simp only [hadv] at hstep ⊢
          apply runPass_preserves_inv L (i + 1) none
            (updateOut out a.opener (some (a.params, i))) (steps + 1) (by omega)
          refine ⟨(fun _ hc => nomatch hc), ?_, ?_⟩
          · intro k hk _
            by_cases hki : k < i
            · by_cases hka : k = a.opener
              · subst hka
                simp [updateOut, hlb, hspec, hstep]
              · have hbeq : (k == a.opener) = false := beq_false_of_ne hka
                simp only [updateOut, hbeq, Bool.false_eq_true, ↓reduceIte]
                exact hdone k hki (fun b hb => by cases hb; exact hka)
            · have heq : k = i := by omega
              subst heq
              have hbeq : (k == a.opener) = false := beq_false_of_ne (by omega)
              have hget : L[k]? ≠ some .leftBrace := by
                simp [List.getElem?_eq_getElem hlt, htok]
              simp [updateOut, hbeq, hfut k (Nat.le_refl _), hget]
          · intro k hk
            have hbeq : (k == a.opener) = false := beq_false_of_ne (by omega)
            simp [updateOut, hbeq, hfut k (by omega)]
  · simp only [hlt, ↓reduceDIte]
    have heq : i = L.length := by omega
    subst heq
    intro k hk
    cases active with
    | none =>
      rcases hinv with ⟨_, hdone, _⟩
      exact hdone k hk (fun _ hc => nomatch hc)
    | some a =>
      rcases hinv with ⟨hact, hdone, _⟩
      rcases hact a rfl with ⟨_, hlb, hnone, hspec⟩
      by_cases hka : k = a.opener
      · subst hka
        rw [hnone, hlb]
        simp only [↓reduceIte]
        rw [hspec]
        unfold specFrom
        simp
      · exact hdone k hk (fun b hb => by cases hb; exact hka)
termination_by i _ _ _ _ _ => L.length - i

theorem singlePass_sound_and_complete (L : List Tok) (k : Nat) (hk : k < L.length) :
    singlePass L k = if L[k]? = some .leftBrace then specHeader L k else none := by
  apply runPass_preserves_inv L 0 none (fun _ => none) 0 (Nat.zero_le _)
  · refine ⟨(fun _ hc => nomatch hc), (fun _ hk' => by omega), (fun _ _ => rfl)⟩
  · exact hk

/-! ### L4: Matched header bounds (`arrow < L.length`, `L[arrow] = .arrow`, params strictly bounded) -/

private theorem specFrom_bounds (L : List Tok) :
    ∀ (i : Nat) (st : TrialState) (acc : List Nat) (params : List Nat) (arrow : Nat),
      (∀ p ∈ acc, p < i) →
      specFrom L i st acc = some (params, arrow) →
      i ≤ arrow ∧ arrow < L.length ∧ L[arrow]? = some .arrow ∧ (∀ p ∈ params, p < arrow) := by
  intro i st acc params arrow hacc hspec
  unfold specFrom at hspec
  split at hspec
  · rename_i hlt
    cases htok : L[i] with
    | trivia =>
      simp only [htok] at hspec
      have ih := specFrom_bounds L (i + 1) st acc params arrow
        (fun p hp => Nat.lt_succ_of_lt (hacc p hp)) hspec
      exact ⟨by omega, ih.2.1, ih.2.2.1, ih.2.2.2⟩
    | ident =>
      simp only [htok] at hspec
      cases st with
      | afterIdent => simp at hspec
      | start | afterComma =>
        simp only at hspec
        have hacc' : ∀ p ∈ acc ++ [i], p < i + 1 := by
          intro p hp
          simp only [List.mem_append, List.mem_singleton] at hp
          rcases hp with hp | rfl
          · exact Nat.lt_succ_of_lt (hacc p hp)
          · omega
        have ih := specFrom_bounds L (i + 1) .afterIdent (acc ++ [i]) params arrow hacc' hspec
        exact ⟨by omega, ih.2.1, ih.2.2.1, ih.2.2.2⟩
    | comma =>
      simp only [htok] at hspec
      cases st with
      | start | afterComma => simp at hspec
      | afterIdent =>
        simp only at hspec
        have ih := specFrom_bounds L (i + 1) .afterComma acc params arrow
          (fun p hp => Nat.lt_succ_of_lt (hacc p hp)) hspec
        exact ⟨by omega, ih.2.1, ih.2.2.1, ih.2.2.2⟩
    | arrow =>
      simp only [htok] at hspec
      cases st with
      | afterComma => simp at hspec
      | start | afterIdent =>
        simp only [Option.some.injEq, Prod.mk.injEq] at hspec
        rcases hspec with ⟨rfl, rfl⟩
        exact ⟨Nat.le_refl _, hlt, by simp [List.getElem?_eq_getElem hlt, htok], hacc⟩
    | leftBrace | other =>
      simp [htok] at hspec
  · simp at hspec
termination_by i _ _ _ _ _ _ => L.length - i

theorem singlePass_header_bounds (L : List Tok) (k : Nat) (hk : k < L.length)
    (params : List Nat) (arrow : Nat)
    (hmatch : singlePass L k = some (params, arrow)) :
    k < arrow ∧ arrow < L.length ∧ L[arrow]? = some .arrow ∧ (∀ p ∈ params, p < arrow) := by
  rw [singlePass_sound_and_complete L k hk] at hmatch
  split at hmatch
  · have hb := specFrom_bounds L (k + 1) .start [] params arrow (by simp) hmatch
    exact ⟨by omega, hb.2.1, hb.2.2.1, hb.2.2.2⟩
  · cases hmatch

/-! ### L5: Linear DFA transition budget (`singlePassSteps L ≤ L.length`) -/

private theorem runPass_steps_le (L : List Tok) :
    ∀ (i : Nat) (active : Option Active) (out : Nat → Option (List Nat × Nat)) (steps : Nat),
      (runPass L i active out steps).2 ≤ steps + (L.length - i) := by
  intro i active out steps
  unfold runPass
  by_cases hlt : i < L.length
  · simp only [hlt, ↓reduceDIte]
    by_cases htriv : (L[i] == .trivia) = true
    · simp only [htriv, ↓reduceIte]
      have ih := runPass_steps_le L (i + 1) active out steps
      omega
    · simp only [htriv, Bool.false_eq_true, ↓reduceIte]
      cases active with
      | none =>
        dsimp only
        have ih := runPass_steps_le L (i + 1)
          (if (L[i] == .leftBrace) = true then some { opener := i, params := [], state := .start } else none)
          out steps
        omega
      | some a =>
        dsimp only
        cases a.advance i L[i] with
        | cont a' =>
          dsimp only
          have ih := runPass_steps_le L (i + 1)
            (if (L[i] == .leftBrace) = true then some { opener := i, params := [], state := .start } else some a')
            out (steps + 1)
          omega
        | failed =>
          dsimp only
          have ih := runPass_steps_le L (i + 1)
            (if (L[i] == .leftBrace) = true then some { opener := i, params := [], state := .start } else none)
            out (steps + 1)
          omega
        | matched =>
          dsimp only
          have ih := runPass_steps_le L (i + 1)
            (if (L[i] == .leftBrace) = true then some { opener := i, params := [], state := .start } else none)
            (updateOut out a.opener (some (a.params, i))) (steps + 1)
          omega
  · simp only [hlt, ↓reduceDIte]
    omega
termination_by i _ _ _ => L.length - i

theorem singlePass_steps_linear (L : List Tok) :
    singlePassSteps L ≤ L.length := by
  have h := runPass_steps_le L 0 none (fun _ => none) 0
  simpa [singlePassSteps] using h

end F6LambdaTrial
