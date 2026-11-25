/-
  SPEC-0500 (F6 fifth slice): Machine-checked unbounded model proofs for
  Pratt expression precedence, associativity, non-associative chain detection,
  and top-level completeness (`crates/lang-frontend/src/parser/engine/expression.rs`,
  `operator.rs`, `engine.rs`).

  Zero external dependencies (Lean 4 core Init only).
  Evidence level: `proved (model, unbounded)` -- NOT full-input production G4.
  Paired with Kani direct-Rust harnesses in `pratt_kani.rs`.
-/

namespace F6PrattPrecedence

inductive NonAssocGroup where
  | range
  | membership
  | comparison
  | equality
  deriving DecidableEq, Repr

inductive Assoc where
  | left
  | right
  | nonAssoc (g : NonAssocGroup)
  deriving DecidableEq, Repr

structure Op where
  prec : Nat
  assoc : Assoc
  deriving DecidableEq, Repr

def Op.rightBp (op : Op) : Nat :=
  match op.assoc with
  | .right => op.prec
  | _ => op.prec + 1

inductive Expr where
  | atom (id : Nat)
  | bin (l : Expr) (op : Op) (r : Expr)
  | chainErr (l : Expr) (op : Op) (r : Expr)
  deriving DecidableEq, Repr

/-- Exact functional model of `parse_expression_bp_inner` over an arbitrary
    infix operator chain, returning both the parsed expression and remaining
    suffix along with a length bound for structural termination. -/
def prattLoop (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool) :
    (tail : List (Op × Nat)) → { res : Expr × List (Op × Nat) // res.2.length ≤ tail.length }
  | [] => ⟨(lhs, []), Nat.le_refl 0⟩
  | (op, rhsAtom) :: rest =>
    if op.prec < minBp then
      ⟨(lhs, (op, rhsAtom) :: rest), Nat.le_refl _⟩
    else
      let rhsStep := prattLoop op.rightBp (.atom rhsAtom) (fun _ => false) rest
      have hlt : rhsStep.val.2.length < ((op, rhsAtom) :: rest).length := by
        have hle := rhsStep.property
        simp only [List.length_cons]
        omega
      let nextStep :=
        match op.assoc with
        | .nonAssoc g =>
          if seen g then
            prattLoop minBp (.chainErr lhs op rhsStep.val.1) seen rhsStep.val.2
          else
            let seen' := fun g' => if g' == g then true else seen g'
            prattLoop minBp (.bin lhs op rhsStep.val.1) seen' rhsStep.val.2
        | _ =>
          prattLoop minBp (.bin lhs op rhsStep.val.1) seen rhsStep.val.2
      ⟨nextStep.val, by
        have hle1 := rhsStep.property
        have hle2 := nextStep.property
        dsimp only [rhsStep] at hle1
        simp only [List.length_cons]
        omega⟩
termination_by tail => tail.length

def prattParse (minBp : Nat) (head : Nat) (tail : List (Op × Nat)) :
    Expr × List (Op × Nat) :=
  (prattLoop minBp (.atom head) (fun _ => false) tail).val

private theorem prattLoop_val_congr (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool)
    {t1 t2 : List (Op × Nat)} (h : t1 = t2) :
    (prattLoop minBp lhs seen t1).val = (prattLoop minBp lhs seen t2).val := by
  subst h; rfl

private theorem prattLoop_nil (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool) :
    (prattLoop minBp lhs seen []).val = (lhs, []) := by
  unfold prattLoop; rfl

private theorem prattLoop_nil_fst (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool) :
    (prattLoop minBp lhs seen []).val.1 = lhs :=
  congrArg Prod.fst (prattLoop_nil minBp lhs seen)

private theorem prattLoop_nil_snd (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool) :
    (prattLoop minBp lhs seen []).val.2 = [] :=
  congrArg Prod.snd (prattLoop_nil minBp lhs seen)

private theorem prattLoop_stop (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool)
    (op : Op) (a : Nat) (rest : List (Op × Nat)) (hlt : op.prec < minBp) :
    (prattLoop minBp lhs seen ((op, a) :: rest)).val = (lhs, (op, a) :: rest) := by
  unfold prattLoop
  simp [hlt]

private theorem prattLoop_stop_fst (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool)
    (op : Op) (a : Nat) (rest : List (Op × Nat)) (hlt : op.prec < minBp) :
    (prattLoop minBp lhs seen ((op, a) :: rest)).val.1 = lhs :=
  congrArg Prod.fst (prattLoop_stop minBp lhs seen op a rest hlt)

private theorem prattLoop_stop_snd (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool)
    (op : Op) (a : Nat) (rest : List (Op × Nat)) (hlt : op.prec < minBp) :
    (prattLoop minBp lhs seen ((op, a) :: rest)).val.2 = (op, a) :: rest :=
  congrArg Prod.snd (prattLoop_stop minBp lhs seen op a rest hlt)

private theorem prattLoop_singleton_ge (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool)
    (op : Op) (a : Nat) (hge : ¬(op.prec < minBp)) :
    (prattLoop minBp lhs seen [(op, a)]).val =
      (match op.assoc with
       | .nonAssoc g => if seen g then .chainErr lhs op (.atom a) else .bin lhs op (.atom a)
       | _ => .bin lhs op (.atom a), []) := by
  unfold prattLoop
  simp only [hge, ↓reduceIte, prattLoop_nil_fst]
  cases h : op.assoc with
  | left =>
    rw [prattLoop_val_congr _ _ _ (prattLoop_nil_snd _ _ _)]
    exact prattLoop_nil _ _ _
  | right =>
    rw [prattLoop_val_congr _ _ _ (prattLoop_nil_snd _ _ _)]
    exact prattLoop_nil _ _ _
  | nonAssoc g =>
    cases hs : seen g <;>
      (simp only [hs, Bool.false_eq_true, ↓reduceIte]; rw [prattLoop_val_congr _ _ _ (prattLoop_nil_snd _ _ _)]; exact prattLoop_nil _ _ _)

private theorem prattLoop_singleton_ge_fst (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool)
    (op : Op) (a : Nat) (hge : ¬(op.prec < minBp)) :
    (prattLoop minBp lhs seen [(op, a)]).val.1 =
      (match op.assoc with
       | .nonAssoc g => if seen g then .chainErr lhs op (.atom a) else .bin lhs op (.atom a)
       | _ => .bin lhs op (.atom a)) :=
  congrArg Prod.fst (prattLoop_singleton_ge minBp lhs seen op a hge)

private theorem prattLoop_singleton_ge_snd (minBp : Nat) (lhs : Expr) (seen : NonAssocGroup → Bool)
    (op : Op) (a : Nat) (hge : ¬(op.prec < minBp)) :
    (prattLoop minBp lhs seen [(op, a)]).val.2 = [] :=
  congrArg Prod.snd (prattLoop_singleton_ge minBp lhs seen op a hge)

/-! ### PR1: Right binding power characterization -/

theorem rightBp_spec (op : Op) :
    (op.assoc = .right → op.rightBp = op.prec) ∧
    (op.assoc ≠ .right → op.rightBp = op.prec + 1) := by
  cases h : op.assoc <;> simp [Op.rightBp, h]

/-! ### PR2: Same-level left-associative operators group left -/

theorem pratt_left_assoc_groups_left (minBp p a b c : Nat) (op1 op2 : Op)
    (h1 : op1.assoc = .left) (h2 : op2.assoc = .left)
    (hp1 : op1.prec = p) (hp2 : op2.prec = p)
    (hmin : minBp ≤ p) :
    prattParse minBp a [(op1, b), (op2, c)] =
      (.bin (.bin (.atom a) op1 (.atom b)) op2 (.atom c), []) := by
  unfold prattParse
  have hnot1 : ¬(op1.prec < minBp) := by omega
  have hlt2 : op2.prec < op1.rightBp := by simp [Op.rightBp, h1, hp1, hp2]
  have hnot2 : ¬(op2.prec < minBp) := by omega
  unfold prattLoop
  simp only [hnot1, ↓reduceIte, h1, prattLoop_stop_fst _ _ _ _ _ _ hlt2]
  rw [prattLoop_val_congr _ _ _ (prattLoop_stop_snd _ _ _ _ _ _ hlt2)]
  simp [prattLoop_singleton_ge _ _ _ _ _ hnot2, h2]

/-! ### PR3: Same-level right-associative operators group right -/

theorem pratt_right_assoc_groups_right (minBp p a b c : Nat) (op1 op2 : Op)
    (h1 : op1.assoc = .right) (h2 : op2.assoc = .right)
    (hp1 : op1.prec = p) (hp2 : op2.prec = p)
    (hmin : minBp ≤ p) :
    prattParse minBp a [(op1, b), (op2, c)] =
      (.bin (.atom a) op1 (.bin (.atom b) op2 (.atom c)), []) := by
  unfold prattParse
  have hnot1 : ¬(op1.prec < minBp) := by omega
  have hnot2 : ¬(op2.prec < op1.rightBp) := by simp [Op.rightBp, h1, hp1, hp2]
  unfold prattLoop
  simp only [hnot1, ↓reduceIte, h1, prattLoop_singleton_ge_fst _ _ _ _ _ hnot2, h2]
  rw [prattLoop_val_congr _ _ _ (prattLoop_singleton_ge_snd _ _ _ _ _ hnot2)]
  exact prattLoop_nil _ _ _

/-! ### PR4: Higher precedence binds tighter in both directions -/

theorem pratt_higher_prec_binds_tighter_right (minBp a b c : Nat) (op1 op2 : Op)
    (hmin : minBp ≤ op1.prec) (hgt : op1.prec < op2.prec) :
    prattParse minBp a [(op1, b), (op2, c)] =
      (.bin (.atom a) op1 (.bin (.atom b) op2 (.atom c)), []) := by
  unfold prattParse
  have hnot1 : ¬(op1.prec < minBp) := by omega
  have hnot2 : ¬(op2.prec < op1.rightBp) := by
    cases h1 : op1.assoc <;> simp [Op.rightBp, h1] <;> omega
  unfold prattLoop
  simp only [hnot1, ↓reduceIte, prattLoop_singleton_ge_fst _ _ _ _ _ hnot2]
  cases h1 : op1.assoc <;> cases h2 : op2.assoc <;>
    (simp only [Bool.false_eq_true, ↓reduceIte]; rw [prattLoop_val_congr _ _ _ (prattLoop_singleton_ge_snd _ _ _ _ _ hnot2)]; exact prattLoop_nil _ _ _)

theorem pratt_higher_prec_binds_tighter_left (minBp a b c : Nat) (op1 op2 : Op)
    (hmin : minBp ≤ op2.prec) (hgt : op2.prec < op1.prec)
    (hdisj : ∀ g, op1.assoc = .nonAssoc g → op2.assoc ≠ .nonAssoc g) :
    prattParse minBp a [(op1, b), (op2, c)] =
      (.bin (.bin (.atom a) op1 (.atom b)) op2 (.atom c), []) := by
  unfold prattParse
  have hnot1 : ¬(op1.prec < minBp) := by omega
  have hlt2 : op2.prec < op1.rightBp := by
    cases h1 : op1.assoc <;> simp [Op.rightBp, h1] <;> omega
  have hnot2 : ¬(op2.prec < minBp) := by omega
  unfold prattLoop
  simp only [hnot1, ↓reduceIte, prattLoop_stop_fst _ _ _ _ _ _ hlt2]
  cases h1 : op1.assoc with
  | left =>
    rw [prattLoop_val_congr _ _ _ (prattLoop_stop_snd _ _ _ _ _ _ hlt2)]
    cases h2 : op2.assoc <;> simp [prattLoop_singleton_ge _ _ _ _ _ hnot2, h2]
  | right =>
    rw [prattLoop_val_congr _ _ _ (prattLoop_stop_snd _ _ _ _ _ _ hlt2)]
    cases h2 : op2.assoc <;> simp [prattLoop_singleton_ge _ _ _ _ _ hnot2, h2]
  | nonAssoc g1 =>
    simp only [Bool.false_eq_true, ↓reduceIte]
    rw [prattLoop_val_congr _ _ _ (prattLoop_stop_snd _ _ _ _ _ _ hlt2)]
    cases h2 : op2.assoc with
    | left => simp [prattLoop_singleton_ge _ _ _ _ _ hnot2, h2]
    | right => simp [prattLoop_singleton_ge _ _ _ _ _ hnot2, h2]
    | nonAssoc g2 =>
      have hne : g2 ≠ g1 := by
        intro heq
        subst heq
        exact hdisj g2 h1 h2
      simp [prattLoop_singleton_ge _ _ _ _ _ hnot2, h2, hne]

/-! ### PR5: Non-associative chain detection & cross-group non-interference -/

theorem pratt_non_assoc_chain_detected (minBp p a b c : Nat) (g : NonAssocGroup) (op1 op2 : Op)
    (h1 : op1.assoc = .nonAssoc g) (h2 : op2.assoc = .nonAssoc g)
    (hp1 : op1.prec = p) (hp2 : op2.prec = p)
    (hmin : minBp ≤ p) :
    prattParse minBp a [(op1, b), (op2, c)] =
      (.chainErr (.bin (.atom a) op1 (.atom b)) op2 (.atom c), []) := by
  unfold prattParse
  have hnot1 : ¬(op1.prec < minBp) := by omega
  have hlt2 : op2.prec < op1.rightBp := by simp [Op.rightBp, h1, hp1, hp2]
  have hnot2 : ¬(op2.prec < minBp) := by omega
  unfold prattLoop
  simp only [hnot1, ↓reduceIte, h1, Bool.false_eq_true, prattLoop_stop_fst _ _ _ _ _ _ hlt2]
  rw [prattLoop_val_congr _ _ _ (prattLoop_stop_snd _ _ _ _ _ _ hlt2)]
  simp [prattLoop_singleton_ge _ _ _ _ _ hnot2, h2]

theorem pratt_distinct_non_assoc_groups_no_chain_error
    (minBp p a b c : Nat) (g1 g2 : NonAssocGroup) (op1 op2 : Op)
    (h1 : op1.assoc = .nonAssoc g1) (h2 : op2.assoc = .nonAssoc g2)
    (hne : g1 ≠ g2)
    (hp1 : op1.prec = p) (hp2 : op2.prec = p)
    (hmin : minBp ≤ p) :
    prattParse minBp a [(op1, b), (op2, c)] =
      (.bin (.bin (.atom a) op1 (.atom b)) op2 (.atom c), []) := by
  unfold prattParse
  have hnot1 : ¬(op1.prec < minBp) := by omega
  have hlt2 : op2.prec < op1.rightBp := by simp [Op.rightBp, h1, hp1, hp2]
  have hnot2 : ¬(op2.prec < minBp) := by omega
  unfold prattLoop
  simp only [hnot1, ↓reduceIte, h1, Bool.false_eq_true, prattLoop_stop_fst _ _ _ _ _ _ hlt2]
  rw [prattLoop_val_congr _ _ _ (prattLoop_stop_snd _ _ _ _ _ _ hlt2)]
  simp [prattLoop_singleton_ge _ _ _ _ _ hnot2, h2, Ne.symm hne]

/-! ### PR6: Unbounded top-level completeness (`minBp = 0` consumes entire chain) -/

theorem prattLoop_zero_consumes_all :
    ∀ (tail : List (Op × Nat)) (lhs : Expr) (seen : NonAssocGroup → Bool),
      (prattLoop 0 lhs seen tail).val.2 = []
  | [], lhs, seen => by
    simp [prattLoop]
  | (op, rhsAtom) :: rest, lhs, seen => by
    unfold prattLoop
    have hnot : ¬(op.prec < 0) := Nat.not_lt_zero _
    simp only [hnot, ↓reduceIte]
    have hlt : (prattLoop op.rightBp (.atom rhsAtom) (fun _ => false) rest).val.2.length <
        ((op, rhsAtom) :: rest).length := by
      have hle := (prattLoop op.rightBp (.atom rhsAtom) (fun _ => false) rest).property
      simp only [List.length_cons]
      omega
    cases h : op.assoc with
    | left =>
      exact prattLoop_zero_consumes_all _ _ _
    | right =>
      exact prattLoop_zero_consumes_all _ _ _
    | nonAssoc g =>
      dsimp only
      split
      · exact prattLoop_zero_consumes_all _ _ _
      · exact prattLoop_zero_consumes_all _ _ _
termination_by tail => tail.length

theorem pratt_top_level_consumes_all (head : Nat) (tail : List (Op × Nat)) :
    (prattParse 0 head tail).2 = [] :=
  prattLoop_zero_consumes_all tail (.atom head) (fun _ => false)

end F6PrattPrecedence
