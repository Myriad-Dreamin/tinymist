/// path: base.typ
/// - body (content): The body of the body
/// -> content
#let todo(body) = body; // redefine just for auto-completion

-----
/// contains: todo
#import "base.typ": todo
#to(/* range -2..0 */ );
