/// Lambda constructor.
///
/// Typing Rule:
///
/// $ (Γ , x : A ⊢ M : B #h(2em) Γ ⊢ a:B)/(Γ ⊢ λ (x : A) → M : π (x : A) → B) $
///
/// - A (type): The type of the argument.
///   - It can be also regarded as the condition of the proposition.
/// - B (type): The type of the body.
///   - It can be also regarded as the conclusion of the proposition.
#let lam(A, B) = (kind: "lambda", args: A, body: B)

#(/* ident after */ lam);
