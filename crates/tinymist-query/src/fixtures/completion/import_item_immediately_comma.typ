/// path: base.typ
#let aa() = 1;
#let bcd() = 2;

-----
/// contains: base,aa,bcd
#import "base.typ": aa,/* range 0..1 */  


