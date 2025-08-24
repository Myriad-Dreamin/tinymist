/// resolve: source.typst.spaceUnknownMathVars

#let proof(body) = [_Proof._ #body]
#proof[
	$sum_(i=0)^100 sum_(j=0)^100 A_ij + sum_(j=0)^100 sum_(k=50)^100 A_jk$
]
