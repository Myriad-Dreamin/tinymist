/// path: base.typ

#let base() = 1;

-----
/// contains: base
#import "base.typ"
#import "base.typ": *
#base(/* range -2..0 */ );
