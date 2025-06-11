
#let typing-item(kind: none, ..args) = (func: typing-item, kind: kind, args: args);

/// An array type (Array<T>).
#let arr(..args) = typing-item.with(kind: "arr", ..args);
/// A tuple type (Tuple<T1, T2, ...>).
#let tuple(..args) = typing-item.with(kind: "tuple", ..args);
/// A dictionary type (Dict<str, V>).
#let dict(..args) = typing-item.with(kind: "dict", ..args);
/// A record type (K1 => V1, K2 => V2, ...).
#let record(..args) = typing-item.with(kind: "record", ..args);

/// A positional parameter.
#let pos(..args) = typing-item.with(kind: "pos", ..args);
/// A named parameter.
#let named(..args) = typing-item.with(kind: "named", ..args);
/// A named parameter.
#let named-required = named.with(required: true);
/// A spread left or right parameter.
#let rest(..args) = typing-item.with(kind: "rest", ..args);

/// A polymorphic signature.
#let sig(..args) = typing-item.with(kind: "sig", ..args);
/// A interface type.
#let interface(..args) = typing-item.with(kind: "interface", ..args);
/// A type.
#let ty(..args) = typing-item.with(kind: "ty", ..args);
/// A type.
#let var(..args) = typing-item.with(kind: "var", ..args);
/// A type variable.
#let tv(..args) = typing-item.with(kind: "tv", ..args);
/// A recursive type.
#let rec(..args) = typing-item.with(kind: "rec", ..args);
/// Adds constraint `A <: B`, and returns `A`.
#let prec(..args) = typing-item.with(kind: "prec", ..args);
/// Adds constraint `A >: B`, and returns `A`.
#let succ(..args) = typing-item.with(kind: "succ", ..args);
/// A union type.
#let union(..args) = typing-item.with(kind: "union", ..args);
/// An intersection type.
#let intersect(..args) = typing-item.with(kind: "intersect", ..args);
/// An invariant type.
#let invariant(..args) = typing-item.with(kind: "invariant", ..args);

/// Op
#let op = typing-item.with(kind: "op");
/// Op add
#let add(..args) = op.with(op: "add", ..args);
/// Op eq
#let eq(..args) = op.with(op: "eq", ..args);
/// Op neq
#let neq(..args) = op.with(op: "neq", ..args);

#let any = var();
#let never = typing-item.with(kind: "never");
#let Self = typing-item.with(kind: "self");
#let satisfy = intersect;
// todo: opt should be sum type
#let opt(T) = union(T, none);
