#set document(title: "Notify-rs File Operation Decomposition")
#set page(width: 20cm, height: auto, margin: 12mm)
#set text(size: 8.4pt)
#set par(justify: false, leading: 0.58em)

#let mono(body) = text(font: "DejaVu Sans Mono", size: 6.6pt, body)
#let key(body) = text(weight: "bold", body)
#let yes = text(weight: "bold", fill: rgb("#006b3f"))[Yes]
#let maybe = text(weight: "bold", fill: rgb("#895100"))[Maybe]
#let no = text(weight: "bold", fill: rgb("#8a1f1f"))[No]
#let light(body) = text(fill: rgb("#555555"), body)
#let mathbox(body) = block(
  width: auto,
  inset: 5pt,
  stroke: 0.45pt + rgb("#cccccc"),
  fill: rgb("#fafafa"),
  body,
)
#let proofbox(body) = block(
  width: 100%,
  inset: 5pt,
  stroke: 0.45pt + rgb("#cccccc"),
  fill: rgb("#fbfbfb"),
  text(font: "DejaVu Sans Mono", size: 5.2pt, body),
)
#let z3-src = read("notify-rs-file-operation-decomposition.z3.py")
#let z3-lines(lo, hi) = z3-src.split("\n").slice(lo - 1, hi).join("\n")
#let z3part(title, lo, hi) = [
  #v(2pt)
  #text(size: 7pt, weight: "bold")[#title]
  #proofbox(raw(z3-lines(lo, hi), lang: "python", block: true))
]

= Notify-rs File Operation Decomposition

This document defines a relation table for non-trivial user file operations that
can affect Tinymist correctness when modeled through notify-rs style events. The
rows are meant to be shared by top-down watcher tests and bottom-up
compiler/analyzer/cache tests.

The scope is intentionally narrower than "all filesystem events". Pure access
events and metadata-only events are excluded unless they are observed together
with content, path, readability, or watch-membership changes.

== Formal Logic Model

The model starts from user operations that can reach Tinymist through a
notify-rs style ingress, then quotients away event noise. The 20 rows below are
the quotient classes of the filtered operation set; they are not examples drawn
from the larger set of all operating-system events.

Let the observable domain be expressed by paths, snapshots, notify-rs atoms, and
Tinymist runtime projections:

#mathbox[$
  U_"fs" = "user filesystem operations visible to Tinymist", \
  P = "absolute paths", \
  B = "bytes", \
  E = "read errors", \
  S = { "ok"(b) | b in B } union { "err"(e) | e in E }, \
  K_0 = { "create", "modify-data", "modify-name-from", "modify-name-to" }, \
  K_1 = { "modify-name-both", "remove", "modify-metadata", "access", "other" }, \
  K = K_0 union K_1, \
  N = K times P^+, \
  R = { "update"(I, D, sigma) | I subset.eq P times S and D subset.eq P and sigma in { "sync", "nonsync" } } union { "upstream"(I, D, U) }
$]

The semantic path relation is intentionally orthogonal to the raw event kind.
The primary topology $tau$ and its three refinements are the only classifiers
used to choose one of the 20 rows:

#mathbox[$
  L = { "entry", "dep", "missing-dep", "retained-inactive-dep", "unrelated", "shadow-open", "asset-dep" }, \
  T_0 = { "create", "content-update", "transient-empty", "read-error" }, \
  T_1 = { "remove", "recreate", "atomic-replace", "rename-file", "rename-dir" }, \
  T_2 = { "move-root", "membership-remove", "membership-add" }, \
  T_3 = { "shadow-fs-race", "symlink-target", "mixed-batch" }, \
  T = T_0 union T_1 union T_2 union T_3, \
  G = { "file", "dir", "subtree", "link", "mixed" }, \
  B_"ref" = { "none", "stale", "updated" }, \
  K_c: U_"fs" -> {0, 1}, \
  Pi = { "server-notify", "delegated-client-watch", "text-sync", "assisted-will-rename" }
$]

#z3part("Z3 mirror: sorts and operation axes", 1, 27)

A concrete user operation is not modeled directly by the operating system event.
It is first normalized into the row-selection axes plus the projected runtime
effects:

#mathbox[$
  nu: U_"fs" -> T times G times B_"ref" times {0, 1} times cal(P)(L) times S^P times N^* times Pi^+ times R^*, \
  nu(u) = (tau(u), g(u), beta(u), K_c(u), lambda_u, omega_u, eta_u, pi_u, rho_u)
$]

Here $tau$ is the primary topology, $g$ is target granularity, $beta$ records
whether references remain stale or are updated with the topology change,
$K_c(u)=1$ marks case-only path equivalence, $lambda_u$ maps relevant paths to Tinymist
relations, $omega_u$ is the post-operation read result, $eta_u$ is the notify-rs
atom sequence, $pi_u$ is the ingress path, and $rho_u$ is the normalized
Tinymist runtime projection.

Only operations that can affect the framework are admitted into the
decomposition. This is the point at which pure access events, timestamp churn,
and irrelevant unrelated edits are removed:

#mathbox[$
  L_"aff" = { "entry", "dep", "missing-dep", "retained-inactive-dep", "shadow-open", "asset-dep" }, \
  M(u) :=
    (exists p in P: lambda_u(p) in L_"aff")
    or (exists p in P: omega_u(p) in { "err"(e) | e in E })
    or tau(u) in { "rename-dir", "move-root", "mixed-batch" }, \
  U_"aff" = { u in U_"fs" | M(u) }
$]

The normalizer also has a well-formedness contract. Combinations outside this
contract are not distinct user-operation classes; they must be normalized first
as symlink-target, directory/subtree operation, or mixed-batch:

#mathbox[$
  W_1(u) := tau(u) = "remove" => g(u) in { "file", "dir", "subtree" }, \
  W_2(u) := tau(u) = "recreate" => g(u) = "file", \
  W_3(u) := tau(u) = "rename-file" and K_c(u) = 0 => beta(u) in { "stale", "updated" }, \
  W_4(u) := tau(u) = "rename-dir" => beta(u) in { "stale", "updated" }, \
  W_5(u) := tau(u) = "move-root" => g(u) in { "file", "dir", "subtree" }, \
  W_"f"(u) := W_1(u) and W_2(u) and W_3(u) and W_4(u) and W_5(u), \
  U_"cov" = { u in U_"aff" | W_"f"(u) }
$]

#z3part("Z3 mirror: normalized universe U_cov", 30, 40)

The direct correspondence to the 20 table rows is the classifier
$chi: U_"cov" -> O$. Labels $o_1$ through $o_20$ correspond to table rows O01
through O20:

#mathbox[$
  O = { o_1, o_2, ..., o_20 }, \
  chi(u) = o_1 <=> tau(u) = "create", \
  chi(u) = o_2 <=> tau(u) = "content-update", \
  chi(u) = o_3 <=> tau(u) = "transient-empty", \
  chi(u) = o_4 <=> tau(u) = "read-error", \
  chi(u) = o_5 <=> tau(u) = "remove" and g(u) = "file", \
  chi(u) = o_6 <=> tau(u) = "recreate" and g(u) = "file", \
  chi(u) = o_7 <=> tau(u) = "atomic-replace", \
  chi(u) = o_8 <=> tau(u) = "rename-file" and beta(u) = "stale" and K_c(u) = 0, \
  chi(u) = o_9 <=> tau(u) = "rename-file" and beta(u) = "updated" and K_c(u) = 0, \
  chi(u) = o_10 <=> tau(u) = "rename-file" and K_c(u) = 1
$]

#z3part("Z3 mirror: O01 through O10", 43, 54)

#mathbox[$
  chi(u) = o_11 <=> tau(u) = "move-root" and g(u) = "file", \
  chi(u) = o_12 <=> tau(u) = "rename-dir" and beta(u) = "stale", \
  chi(u) = o_13 <=> tau(u) = "rename-dir" and beta(u) = "updated", \
  chi(u) = o_14 <=> tau(u) = "remove" and g(u) in { "dir", "subtree" }, \
  chi(u) = o_15 <=> tau(u) = "move-root" and g(u) in { "dir", "subtree" }, \
  chi(u) = o_16 <=> tau(u) = "membership-remove", \
  chi(u) = o_17 <=> tau(u) = "membership-add", \
  chi(u) = o_18 <=> tau(u) = "shadow-fs-race", \
  chi(u) = o_19 <=> tau(u) = "symlink-target", \
  chi(u) = o_20 <=> tau(u) = "mixed-batch"
$]

#z3part("Z3 mirror: O11 through O20", 55, 65)

The no-duplicate/no-omission property is then a property of $chi$, not a
separate informal claim:

#mathbox[$
  O_i := { u in U_"cov" | chi(u) = o_i }, \
  U_"cov" = O_1 union O_2 union ... union O_20, \
  forall i != j: O_i inter O_j = emptyset, \
  forall u in U_"cov": exists! i in { 1, ..., 20 }: u in O_i
$]

#z3part("Z3 mirror: coverage, uniqueness, and inhabited-row checks", 68, 103)

Within a row, relation variants such as entry, dependency, missing dependency,
unrelated file, and opened shadow file are test dimensions. They do not create
new rows unless they change $tau$, $g$, $beta$, or $K_c$. A mixed batch is the
fallback topology for a final-state delta that cannot be represented as one of
O01 through O19 without losing ordering or coalescing obligations.

The correctness obligation for each operation is a conjunction over watcher,
VFS, compiler, and analyzer projections:

#mathbox[$
  C(u) :=
  W(eta_u, rho_u)
  and V(rho_u)
  and G(rho_u)
  and A(rho_u)
  and H(rho_u)
$]

Expanded over the runtime projection, stale path retirement is the central
invariant for rename, remove, and directory-prefix changes:

#mathbox[$
  forall u in U_"cov", forall p_o in P:
  X(u, p_o)
  => (
    p_o in D(rho_u)
    or exists s in S: (p_o, s) in I(rho_u) and s = "err"("not-found")
  )
  => not Z(u, p_o) and not Y(p_o, b_u)
$]

Coverage is also a logical predicate, not only a count of tests:

#mathbox[$
  Q(O_i) :=
  exists t_t: Q_t(t_t, O_i)
  and exists t_b: Q_b(t_b, O_i)
  and exists t_a: Q_a(t_a, O_i), \
  Q_A := forall i in {1, ..., 20}: Q(O_i)
$]

#pagebreak()
#set page(width: auto)

== Normalization Model

Every operation row is normalized into the tuple:

#align(center, table(
  columns: (2.4cm, 5.7cm, 5.9cm, 5.8cm, 5.4cm, 5.5cm),
  inset: 4pt,
  align: left + horizon,
  stroke: 0.45pt + rgb("#cccccc"),
  table.header(
    [Axis], [Meaning], [Representative Values], [Notify-rs Projection], [Runtime Projection], [Correctness Surface]
  ),
  [Target],
  [The filesystem object the user intended to operate on.],
  [file, directory subtree, non-Typst asset, symlink or link-like path],
  [kind plus path list],
  [affected path set],
  [VFS path map, dependency set],

  [Relation],
  [How the target relates to the last successful compilation or current editor state.],
  [entry, active dependency, missing dependency, retained inactive dependency, unrelated, opened shadow],
  [path may or may not be watched],
  [file ids, shadow paths, dependency paths],
  [compiler reason, analysis revision],

  [Topology],
  [Whether the path identity changes.],
  [same path, create, remove, rename, move, directory-prefix rewrite],
  [create, remove, name modify, paired path event, batch],
  [insert, remove, insert plus remove],
  [stale path retirement],

  [Observation],
  [What content or read result Tinymist can observe at the affected paths.],
  [ok bytes, empty bytes, not found, permission/error, unchanged bytes],
  [read after event or delayed recheck],
  [FileSnapshot ok or err],
  [diagnostics, source cache],

  [Batching],
  [Whether the backend emits one atom or a coalesced batch.],
  [single path, old plus new, multi-file subtree, temp plus final],
  [one Event or several Events],
  [one or several FileChangeSet values],
  [ordering and debounce],

  [Client Path],
  [Which ingress path delivers the observation.],
  [server watcher, delegated client watcher, didOpen/didChange shadow, assisted rename request],
  [notify event or client request],
  [FilesystemEvent or MemoryEvent],
  [LSP-visible behavior],
))

The table below is mutually exclusive by #key[primary topology class]. Within a
row, relation variants such as entry, dependency, and unrelated file are not new
operation classes; they are test dimensions that must be instantiated for the
same primary operation.

== Notify-rs Event Atom Projection

#align(center, table(
  columns: (3.4cm, 6.4cm, 6.0cm, 6.4cm, 6.4cm),
  inset: 4pt,
  align: left + horizon,
  stroke: 0.45pt + rgb("#cccccc"),
  table.header([Atom], [Typical User Operation], [Important Variants], [Tinymist Runtime Shape], [Test Note]),
  [#mono[`Create(_)`]],
  [New file appears, temp file appears, moved-to path appears on backends that do not emit paired rename.],
  [file, directory, any; watched or not watched],
  [Usually #mono[`insert(path, Ok|Err)`] after read.],
  [Only correctness-relevant if the path is entry, dependency, newly referenced, or part of a directory move batch.],

  [#mono[`Modify(Data(_))`]],
  [Content write, save, overwrite, asset update.],
  [non-empty, empty, unchanged, read error],
  [#mono[`insert(path, snapshot)`] or no update if unchanged.],
  [Must distinguish stable content from transient empty or transient read error.],

  [#mono[`Modify(Name(From))`]],
  [Rename-away or move-away old path.],
  [single old path; may be followed by #mono[`To`] or #mono[`Both`]],
  [old path becomes #mono[`Err/NotFound`] after recheck, or #mono[`remove(old)`] in mock/client projection.],
  [This is the stale-cache path-retirement trigger.],

  [#mono[`Modify(Name(To))`]],
  [Rename-to or move-to new path.],
  [single new path; sometimes emitted as create],
  [#mono[`insert(new, snapshot)`].],
  [Only enough for recovery if references already point to the new path.],

  [#mono[`Modify(Name(Both))`]],
  [Backend emits old and new paths in one atom.],
  [two paths, sometimes more for subtree events],
  [#mono[`remove(old) + insert(new)`] after normalization.],
  [Must be represented even if a platform normally emits From plus To.],

  [#mono[`Remove(_)`]],
  [Delete file, delete watched dependency, delete subtree, atomic save old path removal.],
  [file, directory, any; watched or retained inactive],
  [#mono[`remove(path)`] or #mono[`insert(path, Err)`] depending on ingress.],
  [Deletion through delegated watcher currently arrives as read error unless explicit removes are sent.],

  [#mono[`Modify(Metadata(_))`]],
  [Permission, owner, timestamp, mode, executable bit.],
  [metadata-only, permission-to-readability change],
  [No runtime change for pure metadata; #mono[`insert(path, Err|Ok)`] when readability changes.],
  [Exclude pure timestamp churn; include permission/readability transitions.],

  [#mono[`Access(_)`] or #mono[`Other`]],
  [Open, close, editor bookkeeping.],
  [usually irrelevant],
  [Usually empty runtime projection.],
  [Only include when a backend coalesces it with relevant atoms.],
))

== Non-trivial User Operation Decomposition

#align(center, table(
  columns: (0.85cm, 2.95cm, 3.25cm, 3.6cm, 3.8cm, 4.2cm, 4.1cm, 4.15cm, 4.0cm),
  inset: 3.6pt,
  align: left + horizon,
  stroke: 0.42pt + rgb("#cccccc"),
  table.header(
    [ID],
    [Primary Class],
    [User Operation],
    [Path Relations to Instantiate],
    [Notify-rs Atoms],
    [Runtime Projection],
    [Required Framework Response],
    [Top-down Tests],
    [Bottom-up Tests],
  ),
  [O01],
  [Create],
  [Create a file at a previously absent path.],
  [missing dependency, newly referenced dependency, unrelated, entry from empty workspace],
  [#mono[`Create(File)`], sometimes #mono[`Modify(Data)`]],
  [#mono[`insert(new, Ok)`] or #mono[`insert(new, Err)`] if read fails],
  [Recover missing imports when referenced; keep unrelated create harmless; add watch only if it becomes a dependency.],
  [real watcher create; delegated watcher create; created dependency after failed compile],
  [VFS insert; project create-dependency row; analysis query after recovery],

  [O02],
  [Content Update],
  [Modify bytes at an existing path.],
  [entry, active dependency, non-Typst asset dependency, unrelated],
  [#mono[`Modify(Data(Content|Any))`], sometimes #mono[`Any`]],
  [#mono[`insert(path, Ok(new))`], or no change if bytes unchanged],
  [Invalidate cached bytes and parsed source; compile if entry or dependency; keep unrelated churn harmless.],
  [real watcher write; delegated watcher change; save workaround],
  [VFS source replacement; project edit-entry and edit-dependency rows; semantic queries use new revision],

  [O03],
  [Transient Empty],
  [Editor truncates file to empty before writing final contents.],
  [active dependency, entry, unrelated],
  [#mono[`Modify(Data)`] followed by another #mono[`Modify(Data)`]],
  [deferred #mono[`insert(path, Ok(\"\"))`] or recovered #mono[`insert(path, Ok(final))`]],
  [Delay confirmation for watched dependencies; recover before recheck without surfacing empty source.],
  [real watcher empty-write recovery; actor delayed recheck],
  [VFS empty snapshot; project transient-empty row; diagnostics appear only if confirmed],

  [O04],
  [Readability Change],
  [File remains at same path but read result changes to or from error.],
  [entry, active dependency, asset dependency],
  [#mono[`Modify(Metadata)`], #mono[`Modify(Data)`], or backend-specific #mono[`Any`]],
  [#mono[`insert(path, Err)`] then #mono[`insert(path, Ok)`]],
  [Surface unavailable diagnostics, then clear after recovery; do not keep old source cache alive.],
  [permission/read-error smoke where platform allows; delegated read failure],
  [VFS error snapshot; project failed-read-then-recovery row; analysis cache revision check],

  [O05],
  [Remove File],
  [Delete one existing file.],
  [entry, active dependency, retained inactive dependency, unrelated],
  [#mono[`Remove(File)`], sometimes #mono[`Modify(Name(From))`]],
  [#mono[`remove(path)`] or #mono[`insert(path, Err/NotFound)`]],
  [Retire depended path; mark compiler affected; stale raw events for inactive retained deps must not emit.],
  [real watcher delete; delegated watcher delete; late event after unwatch],
  [VFS remove; project remove-dependency row; analysis queries report missing or no result],

  [O06],
  [Delete-Then-Recreate],
  [Delete a file and create a new file at the same path.],
  [active dependency, missing dependency, entry],
  [#mono[`Remove`] then #mono[`Create`] or coalesced modify],
  [#mono[`remove(path)`] then #mono[`insert(path, Ok)`]],
  [First report unavailable if observed, then recover; new bytes must not compare against stale old snapshot incorrectly.],
  [real watcher bounded sequence; delegated delete/create sequence],
  [project delete-then-recreate row; VFS changed_at and source cache assertions],

  [O07],
  [Atomic Replace],
  [Write temp file then replace or rename over final path.],
  [entry, active dependency, asset dependency],
  [temp #mono[`Create`], temp #mono[`Modify`], final #mono[`Modify(Name)`] or final #mono[`Modify(Data)`]],
  [final #mono[`insert(final, Ok)`], or #mono[`remove+insert`] if observed],
  [Ignore unrelated temp path; emit final #mono[`insert(final, Ok)`] or remove-plus-insert if observed.],
  [real watcher atomic replace; save from common editors],
  [combine changes into final FileChangeSet; compile sees final content only],
  [O08],

  [Rename File, References Stale],
  [Rename or move one file while imports still point to the old path.],
  [previously depended path, unrelated moved file],
  [#mono[`Name(From)`] plus #mono[`Name(To)`], #mono[`Name(Both)`], or #mono[`Remove+Create`]],
  [#mono[`remove(old) + insert(new)`], or old #mono[`Err`] plus new #mono[`Ok`]],
  [Old dependency must be unavailable; compiler must not use cached old source; new path may exist but remain unreferenced.],
  [real watcher rename-away; delegated delete/create; unassisted filesystem rename],
  [VFS rename; project rename-stale row; analysis old URI no longer resolves],
  [O09],

  [Rename File, References Updated],
  [Rename or move one file and update imports in the same user transaction.],
  [old dependency, newly referenced dependency, entry edit],
  [#mono[`Name(From/To|Both)`] plus entry #mono[`Modify(Data)`]],
  [#mono[`remove(old) + insert(new) + insert(entry)`]],
  [Follow new path, drop old dependency, keep diagnostics clear, update symbols and references to new URI.],
  [workspace/willRenameFiles plus real FS rename; delegated batch],
  [project rename-updated and rename-batch rows; references/definition queries],
  [O10],

  [Case-only Rename],
  [Rename only letter case, such as #mono[`a.typ`] to #mono[`A.typ`].],
  [entry, active dependency],
  [platform-dependent #mono[`Name(Both)`], #mono[`From+To`], or no event on case-insensitive backend until refresh],
  [#mono[`remove(old) + insert(new)`] where old and new may compare equal on some filesystems],
  [Normalize paths carefully; avoid double-retaining stale source; verify per platform.],
  [ignored real watcher on Linux/macOS/Windows; delegated watcher URI case behavior],
  [mock VFS case-sensitive row plus platform-specific integration],
  [O11],

  [Move File Across Root Boundary],
  [Move a file into, out of, or across workspace roots.],
  [dependency moved out, newly visible dependency moved in, unrelated outside root],
  [#mono[`Name(From/To|Both)`], create-only, remove-only depending on watched scope],
  [remove-only, insert-only, or remove-plus-insert],
  [Moved-out dependency becomes unavailable; moved-in dependency can recover only if resolver can address it.],
  [real watcher root-boundary move; delegated watcher workspace folder boundary],
  [resolver and VFS path-map tests; compiler dependency retirement],
  [O12],

  [Rename Directory, References Stale],
  [Rename or move a directory containing one or more depended files while imports still use old prefix.],
  [multiple previously depended paths, unrelated children],
  [backend-specific subtree batch: parent #mono[`Name`] plus child creates/removes, or only child events],
  [multi #mono[`remove(old/*) + insert(new/*)`] or partial remove/error set],
  [Every previously depended child path under old prefix must retire; unrelated children must not force extra recompiles.],
  [real watcher directory rename; delegated subtree delete/create behavior],
  [mock subtree rename helper; project multi-dependency stale-prefix rows],
  [O13],

  [Rename Directory, References Updated],
  [Rename or move a directory and update all relevant imports.],
  [old dependency prefix, new dependency prefix, entry edits across importers],
  [subtree #mono[`Name`] plus importer #mono[`Modify(Data)`]],
  [multi #mono[`remove(old/*) + insert(new/*) + insert(importers)`]],
  [Follow new prefix for all imported files; dependency sync must not retain old subtree paths.],
  [assisted or manual bulk rename; real watcher subtree event variability],
  [mock prefix rewrite; project compile and references across multiple modules],
  [O14],

  [Delete Directory],
  [Delete a directory containing depended files.],
  [one or many active dependencies, unrelated children, entry if directory contains entry],
  [parent #mono[`Remove(Dir)`], child #mono[`Remove(File)`], or partial backend event set],
  [multi #mono[`remove(path)`] or #mono[`insert(path, Err)`] for known watched children],
  [All last-known depended children become unavailable; no stale source from deleted subtree.],
  [real watcher directory delete; delegated watcher recursive delete],
  [project multi-remove; VFS path-map invalidation for each known child],
  [O15],

  [Move Directory Across Root Boundary],
  [Move a subtree into or out of the workspace or project root.],
  [dependency subtree, package-like subtree, unrelated subtree],
  [partial create/remove/name events based on watched scope],
  [multi insert-only, remove-only, or remove-plus-insert],
  [Resolver, root restriction, dependency retirement, and diagnostics must agree.],
  [real watcher root-boundary subtree move],
  [resolver-focused mock world tests; compile cache retirement],
  [O16],

  [Dependency Membership Removal],
  [Imports change so a watched file is no longer a dependency, without changing that file.],
  [active dependency becomes retained inactive],
  [entry #mono[`Modify(Data)`] followed by dependency sync],
  [no direct FileChangeSet for removed dependency; watcher unwatch command],
  [Late raw events or delayed rechecks for inactive retained path must not emit updates.],
  [actor ordering barrier plus late raw event; real watcher stale event test],
  [notify actor retained-entry tests; compiler deps sync check],
  [O17],

  [Dependency Re-addition],
  [A previously removed dependency becomes referenced again.],
  [retained inactive dependency becomes active dependency],
  [entry #mono[`Modify(Data)`] and dependency sync; optional file change while inactive],
  [sync #mono[`insert(path, Ok)`] only if observed content differs],
  [Re-watch path; compare against retained snapshot; emit sync update if content changed while inactive.],
  [actor re-addition; real watcher write-while-unwatched then re-add],
  [notify actor sync row; project dependency recovery; analysis queries fresh],
  [O18],

  [Opened Shadow With Filesystem Change],
  [File is open and edited in memory while filesystem also changes, moves, or deletes it.],
  [opened entry, opened dependency, shadow path removed on close],
  [LSP text sync plus notify/delegated FS event],
  [#mono[`MemoryEvent`] plus #mono[`FilesystemEvent::UpstreamUpdate`] or ordinary FS update],
  [Memory overlay ordering must be deterministic; closing file must reveal current filesystem state, not stale cache.],
  [LSP didOpen/didChange/didClose with real FS mutation],
  [upstream invalidation row; VFS shadow reset/remove tests; analysis revision lock check],
  [O19],

  [Symlink or Link Target Change],
  [Rename, replace, or retarget a symlink-like path used by a project.],
  [dependency path, target path, unrelated target],
  [metadata/name/remove/create events vary by platform and watch target],
  [read result at watched path changes or disappears],
  [Correctness follows observable path content; do not assume target events are watched unless explicitly depended on.],
  [platform-specific ignored watcher tests],
  [VFS path resolution and read-result tests where supported],
  [O20],

  [Mixed Batch],
  [Bulk refactor, git checkout, branch switch, archive extraction, or generated output replacing many files.],
  [entry, many dependencies, unrelated, removed, created, renamed],
  [coalesced #mono[`Any`], many create/modify/remove/name events, or overflow-like backend behavior],
  [multi FileChangeSet; possibly empty event followed by sync],
  [Order-insensitive invariants: no stale depended path, unrelated churn harmless, final compile matches final workspace.],
  [real watcher bounded bulk operation; delegated multi-read request],
  [property-style mock batches built from O01-O19 atoms],
))

== LSP and Cache Impact Matrix

#align(center, table(
  columns: (3.3cm, 5.2cm, 6.2cm, 6.2cm, 6.0cm),
  inset: 4pt,
  align: left + horizon,
  stroke: 0.45pt + rgb("#cccccc"),
  table.header([Surface], [Affected By Rows], [Expected Invariant], [Primary Cache or State], [Minimum Probe]),
  [Compilation],
  [O01-O18, O20],
  [Entry and dependencies compile from current observable workspace; retired paths cannot be silently clean.],
  [#mono[`ProjectInsState::latest_compilation`], #mono[`cached_snapshot`], compile reason],
  [compile after each row; inspect diagnostics and dependency sync],

  [VFS],
  [O01-O15, O18-O20],
  [#mono[`notify_fs_changes`] invalidates known paths and file ids before later reads.],
  [path map, managed entry bytes, parsed Source cache],
  [read #mono[`source(old)`] and #mono[`source(new)`] across rename/remove/recreate],

  [Analysis],
  [O01-O18, O20],
  [Query snapshot revision must advance with VFS or memory changes; results must not mention stale old URI unless reporting missing import.],
  [#mono[`AnalysisRevCache`], local module caches, global signature/docstring caches],
  [hover, definition, references, completion, document symbol, semantic tokens],

  [Semantic Tokens],
  [O02, O07-O10, O12-O13, O18, O20],
  [Full and delta tokens use the active source and path-specific token cache.],
  [#mono[`SemanticTokenCache`]],
  [full then delta after rename/edit; old result id must not resurrect old file],

  [Diagnostics],
  [O01, O03-O06, O08-O15, O18-O20],
  [Missing/read-error diagnostics appear and recover according to final observable state.],
  [compiler diagnostics plus lint result cache],
  [publish diagnostics after missing, then after recovery],

  [Rename Assistance],
  [O08-O13],
  [#mono[`workspace/willRenameFiles`] may update Typst imports for file renames, but runtime correctness must not depend on it.],
  [rename query, workspace edit, source graph],
  [assisted rename and unassisted filesystem rename variants],

  [Watcher State],
  [O01, O05-O17, O19-O20],
  [Watch and unwatch commands match current dependency membership; retained inactive entries are non-emitting.],
  [NotifyActor watched entries, #mono[`seen`], #mono[`watching`], delayed recheck queue],
  [actor harness commands and emitted events],

  [Delegated Client Watch],
  [O01-O15, O20],
  [Client-side read results normalize to the same runtime shape as server-side watcher results.],
  [VS Code watch set, read clock, #mono[`tinymist/fsChange`]],
  [create/change/delete/rename folder through delegated watcher],
))

== Coverage Rule

A row is covered only when all three statements hold:

- A top-down test proves at least one real ingress path can observe the user
  operation and produce the normalized runtime shape.
- A bottom-up test feeds the normalized runtime shape into the owning layer and
  asserts the framework invariant without depending on watcher timing.
- At least one analysis-facing probe confirms that revision-scoped analysis
  state cannot reuse stale source, dependency, or path data.

If a row is not executable on every platform, the row should remain in the
matrix and the platform-specific limitation should be attached to the test,
not removed from the model.

== Existing Implementation Anchors

#align(center, table(
  columns: (4.6cm, 9.5cm, 13.2cm),
  inset: 4pt,
  align: left + horizon,
  stroke: 0.45pt + rgb("#cccccc"),
  table.header([Area], [Anchor], [Reason]),
  [Server-side watcher],
  [#mono[`crates/tinymist-project/src/watch.rs`]],
  [Maps notify-rs raw events and dependency sync messages into #mono[`FilesystemEvent`].],

  [Runtime compiler],
  [#mono[`crates/tinymist-project/src/compiler.rs`]],
  [Consumes #mono[`Interrupt::Fs`] and applies VFS updates before compile scheduling.],

  [VFS invalidation],
  [#mono[`crates/tinymist-vfs/src/lib.rs`]],
  [Owns path/file-id invalidation, read cache, parsed source cache, and #mono[`is_clean_compile`].],

  [Mock operation source],
  [#mono[`crates/tinymist-vfs/src/mock.rs`]],
  [Builds deterministic create, update, remove, and rename #mono[`FileChangeSet`] shapes.],

  [Analysis cache],
  [#mono[`crates/tinymist-query/src/analysis/global.rs`]],
  [Owns revision-managed analysis cache and semantic token cache acquisition.],

  [Delegated watcher],
  [#mono[`editors/vscode/src/lsp.ts`]],
  [Reads watched client files and sends #mono[`tinymist/fsChange`] to the server.],

  [Rename assistance],
  [#mono[`crates/tinymist-query/src/will_rename_files.rs`]],
  [Computes workspace edits for file rename imports; not a runtime invalidation guarantee.],
))
