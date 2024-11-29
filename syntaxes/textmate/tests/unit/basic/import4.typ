
#import"ident.typ";
#import "ident.typ":(a.b, b);
#import "ident.typ" as x:(a.b, b);
#import"ident.typ"as x:(/* test */ a.b,
/* test */ b /* test */, /* test */);
#import("ident.typ");
#import { "ident.typ" };
#import { "ident.typ" }:(a, b);
#import { "ident.typ" }:(
  a, b);
#import { "ident.typ" }:(a.b, b);