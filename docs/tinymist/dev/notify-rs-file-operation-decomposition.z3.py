from z3 import *


# Domain sorts and symbolic operation axes.
Tau, tau_values = EnumSort("Tau", [
    "Create", "ContentUpdate", "TransientEmpty", "ReadError", "Remove",
    "Recreate", "AtomicReplace", "RenameFile", "RenameDir", "MoveRoot",
    "MembershipRemove", "MembershipAdd", "ShadowFsRace", "SymlinkTarget",
    "MixedBatch",
])
(Create, ContentUpdate, TransientEmpty, ReadError, Remove, Recreate,
 AtomicReplace, RenameFile, RenameDir, MoveRoot, MembershipRemove,
 MembershipAdd, ShadowFsRace, SymlinkTarget, MixedBatch) = tau_values

Granularity, granularity_values = EnumSort("Granularity", [
    "File", "Dir", "Subtree", "Link", "Mixed",
])
File, Dir, Subtree, Link, Mixed = granularity_values

RefMode, ref_values = EnumSort("RefMode", ["NoneRef", "Stale", "Updated"])
NoneRef, Stale, Updated = ref_values

tau = Const("tau", Tau)
g = Const("g", Granularity)
beta = Const("beta", RefMode)
case_only = Bool("case_only")
impact = Bool("impact")


# Normalizer well-formedness contract defining U_cov.
wf = And(
    Implies(tau == Remove, Or(g == File, g == Dir, g == Subtree)),
    Implies(tau == Recreate, g == File),
    Implies(
        And(tau == RenameFile, Not(case_only)),
        Or(beta == Stale, beta == Updated),
    ),
    Implies(tau == RenameDir, Or(beta == Stale, beta == Updated)),
    Implies(tau == MoveRoot, Or(g == File, g == Dir, g == Subtree)),
)


# Row predicates O01..O20.
rows = [
    tau == Create,
    tau == ContentUpdate,
    tau == TransientEmpty,
    tau == ReadError,
    And(tau == Remove, g == File),
    And(tau == Recreate, g == File),
    tau == AtomicReplace,
    And(tau == RenameFile, beta == Stale, Not(case_only)),
    And(tau == RenameFile, beta == Updated, Not(case_only)),
    And(tau == RenameFile, case_only),
    And(tau == MoveRoot, g == File),
    And(tau == RenameDir, beta == Stale),
    And(tau == RenameDir, beta == Updated),
    And(tau == Remove, Or(g == Dir, g == Subtree)),
    And(tau == MoveRoot, Or(g == Dir, g == Subtree)),
    tau == MembershipRemove,
    tau == MembershipAdd,
    tau == ShadowFsRace,
    tau == SymlinkTarget,
    tau == MixedBatch,
]


# Proof queries.
def row_count():
    return Sum([If(row, 1, 0) for row in rows])


def check_unsat(name, *constraints):
    solver = Solver()
    solver.add(*constraints)
    result = solver.check()
    print(f"{name}: {result}")
    assert result == unsat, solver.model()


def check_sat(name, *constraints):
    solver = Solver()
    solver.add(*constraints)
    result = solver.check()
    print(f"{name}: {result}")
    assert result == sat
    print(solver.model())


universe = And(impact, wf)

check_unsat("no omitted row in U_cov", universe, row_count() == 0)
check_unsat("no duplicated row in U_cov", universe, row_count() > 1)
check_unsat("not exactly one row in U_cov", universe, row_count() != 1)

for index, row in enumerate(rows, start=1):
    solver = Solver()
    solver.add(universe, row)
    result = solver.check()
    print(f"O{index:02d} inhabited: {result}")
    assert result == sat

check_sat("counterexample without WF", impact, row_count() != 1)
