#import "mod.typ": *

#show: book-page.with(title: "Tinymist LSP Inputs")

== Prefer to Using LSP Configurations

Though tinymist doesn't refuse to keep state in your disk, it actually doesn't have any data to write to disk yet. All customized behaviors (user settings) are passed to the server by LSP configurations. This is a good practice to keep the server state clean and simple.

== Handling Compiler Input Events

The compilation triggers many side effects, but the behavior of compiler actor is still easy to predicate. This is achieved by accepting all compile inputs by events.

Let us take reading files from physical file system as example of processing compile inputs, as shown in @fig:overlay-vfs. The upper access models take precedence over the lower access models. The memory access model is updated _sequentially_ by `LspActor` receiving source change notifications, assigned with logical ticks $t_(L,n)$. The notify access model is also updated in same way by `NotifyActor`. When there is an absent access, the system access model initiates the request for the file system directly. The read contents from fs are assigned with logical access time $t_(M,n)$.

#let pg-hori-sep = 1.5
#let pg-vert-sep = 0.7
#let pg-adjust = 18pt
#figure(
  align(
    center,
    move(
      dx: pg-adjust,
      diagram(
        edge-stroke: 0.85pt,
        node-corner-radius: 3pt,
        edge-corner-radius: 4pt,
        mark-scale: 80%,
        node((pg-hori-sep, +pg-vert-sep), [SystemAccessModel], fill: colors.at(1)),
        node((pg-hori-sep, 0), align(center)[`NotifyAccessModel`], fill: colors.at(1)),
        node((pg-hori-sep, -pg-vert-sep), [MemoryAccessModel], fill: colors.at(1)),
        node((0, 0), align(center)[`NotifyActor`], fill: colors.at(0)),
        node((0, -pg-vert-sep), align(center)[`LspActor`], fill: colors.at(0)),
        edge((0, 0), (pg-hori-sep, 0), "-}>"),
        edge((0, -pg-vert-sep), (pg-hori-sep, -pg-vert-sep), "-}>"),
        edge(
          (-1, -pg-vert-sep),
          (0, -pg-vert-sep),
          "-}>",
          [didChange, \ didOpen, etc.],
          label-anchor: "center",
          label-pos: 0,
        ),
        edge(
          (-0.8, pg-vert-sep),
          (0, pg-vert-sep),
          (0, 0),
          "-}>",
          [readFile\ readDir, etc.],
          label-anchor: "center",
          label-pos: 0,
        ),
        edge((-1, pg-vert-sep), (pg-hori-sep, pg-vert-sep), "-}>"),
        edge((pg-hori-sep, 0), (pg-hori-sep, pg-vert-sep), "-}>"),
        edge((pg-hori-sep, -pg-vert-sep), (pg-hori-sep, 0), "-}>"),
        edge(
          (pg-hori-sep * 1.59, -pg-vert-sep * 1.6),
          (pg-hori-sep, -pg-vert-sep * 1.6),
          (pg-hori-sep, -pg-vert-sep),
          "-}>",
          [sourceOf(path)],
          label-pos: 0.2,
        ),
        for i in (-1, 0, 1) {
          edge(
            (pg-hori-sep * 1.2, i * pg-vert-sep),
            (pg-hori-sep * 1.7, i * pg-vert-sep),
            "-}>",
            [source],
            label-pos: 1,
          )
        },
        node(
          (-1.3, 0),
          rotate(-90deg, rect(stroke: (bottom: (thickness: 1pt, dash: "dashed")), width: 120pt)[Input Sources]),
        ),
        node(
          (pg-hori-sep + 1.45, 0),
          rotate(
            90deg,
            move(
              dy: pg-adjust * 2,
              rect(stroke: (bottom: (thickness: 1pt, dash: "dashed")), width: 120pt)[Compiler World],
            ),
          ),
        ),
      ),
    ),
  ),
  caption: [The overlay virtual file system (VFS)],
) <fig:overlay-vfs>

The problem is to ensure that the compiler can read the content correctly from access models at the time.

If there is an only active input source in a _small time window_, we can know the problem is solved, as the logical ticks $t_(L,n)$ and $t_(M,n)$ keep increasing, enforced by actors. For example, if there is only `LspActor` active at the _small time window_, the memory access model receives the source changes in the order of $t_(L,n)$, i.e. the _sequential_ order of receiving notifications. The cases of two rest access models is more complicated, but are also ensured that compiler reads content in order of $t_(M,n)$.

Otherwise, the two input sources are both active in a _small time window_ on a *same file*. However, this indicates that, the file is in already the memory access model at most time. Since the precedence, the compiler reads content in order of $t_(L,n)$ at the time.

The only bad case can happen is that: When the two input sources are both active in a _small time window_ $delta$ on a *same file*:
- first `LspActor` removes the file from the memory access model, then compiler doesn't read content from file system in time $delta$.
- first `NotifyActor` inserts the file from the inotify thread, then the LSP client (editor) overlays an older content in time $delta$.

This is handled by tinymist by some tricks.

=== Record and Replay

Tinymist can record these input events with assigned the logic ticks. By replaying the events, tinymist can reproduce the server state for debugging. This technique is learnt from the well-known LSP, clangd, and the well known emulator, QEMU.
