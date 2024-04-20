I have implemented nearly all LSP features. The implementations are incomplete but bring some insights. The requirement of analysis may unveil some shortcomings to building incremental analysis (computation) for typst-ide with existing crates, e.g. comemo.

First, to get LSP features, we have two approaches to extract information from Typst's source code:
- dynamic analysis, that triggers `analyze_{expr,import,...}`, which is heavily used by current typst-ide.
- static analysis, which iterates `SyntaxNode/`LinkedNode/Content/Frame`, which is also used by the current typst-ide but no cache is taken.

I list some typical ways to get LSP features. "Go to function definition" can be implemented partially with dynamic analysis, but "Go to variable definition" looks impossible, so we need to have static analysis eventually. "Go to references" initially acquires both dynamic and static analysis. "Inlay hint" can be easily implemented with dynamic analysis, but it has really poor performance since a single inlay hint request triggers many compilations.

The dynamic analysis part in Typst is attractive in comparison with other programming languages from my view. We can usually get nice and wonderful analysis results (autocompletion) by tracking some span to get corresponding values dynamically, but it is not perfect.
- Lossy information: For a non-function value, we cannot track its definition (assignment) site currently.
- Performance overhead: A single compilation is proven to be fast, but to get the best analysis result we may trigger multiple times of compilation and get exhausted memory and long time (observed in implemented inlay hint and goto references feature).

My solution is to introduce more static analysis and cache them heavily. But I find it problematic to build one with comemo. There are limitations observed for implementing performant static analysis.
- Bad performance is caused by hashing, especially on recursive structures. comemo compares input and constraints by hash, which is inefficient enough. I encounter the following cases frequently:
  ```rs
  #[comemo::memorize]
  fn analyze(input: SomeRecursiveStructure) -> Output {}
  ```
  It is elegant, but causes poor performance frequently, as the CPUs are tired of calculating hashes on the recursive structures repeatedly. Specifically, the leaf will be hashed by `O(d)` times, where `d` is the number of wrapped `LazyHash` from the root node to the leaf node. Even though we have `LazyHash`, the overhead is not acceptable when the calculation is cheaper than hashing the input. 
- Lack of revision management (comemo is global), hence causing poor memory efficiency. Now we have a keep-running compilation task along with multiple incoming IDE tasks at some random times, how do we run the global `comemo::evict` for best performance? When we trigger `comemo::evict` frequently for each IDE task, the compilation caches all go away.
- Cannot do task cancellation when analyses are nested by `comemo::memorized`:
  - When some expensive analysis task is running in the background, how do we cancel if it is nested in deep `comemo::memorized`? The cancellation will cause "side effects".
  - This problem also occurs with a long-time compilation for canceling.

I compared salsa (used by rust analyzer) and comemo, and found salsa solves some problems, but I'm not yet sure whether we should use salsa for caching analyses and It is also not actively maintained.

---

I think we have reached some consensus and can land a basic go-to definition feature. For the comemo part, we can just keep discussing and seek opportunities for optimization if possible.

> I think by extending Scope (storing a span) and the Tracer a bit, we could maybe solve that. Though intra-file go-to-definition is also fairly simple on the syntax tree I think, and more efficient. So, we could also go with a hybrid approach: Intra-file syntax-based and inter-file using dynamic analysis on the module and checks the span in the Scope.
This is what I'm doing. The approach may not fit in some other LSP features, but is totally fine with a go to definition feature. We can forget other features first.

> Maybe we can let the Tracer trace more things in a single compilation, so that you can run various analyses at once?
We can do it, but I'm worrying whether it will affect performance on regular compilation. Furthermore, there are some cases will extremely extend time to dynamic analysis. I can give a simple example:
```js
for i in range(5000000) {}
```

When you try to get a `typst::foundations::Func` instance for the `range` function by `analyze_expr`, it will cost 11s on my computer! IMO we should prefer static analysis whenever possible, even if the dynamic analysis is quite wonderful, as we may not like to get user reports that they randomly click some identifier for definition and the browser stucks or crashes... The performance and static analysis are usually more reliable and predictable.

> If you have intermediate LazyHash structures, every leaf should be hashed just once
For example, let's think of an extreme case: `pub enum Either { A(Box<Either>), B }`, and the analysis:

```rs
#[comemo::memorize]
fn analyze(input: Either) -> Output {}
```

We have several simple options:

+ doesn't use lazy hash inside `pub enum Either { A(Box<Either>), B }`, then the `analyze` function will have poor performance when the `Either` is big.
+ intrude a lazy hash inside `pub enum Either { A(Box<LazyHash<Either>>), B }`, then the `analyze` function will trigger for `d` times if the depth of `B` is `d`.
+ add an extra variant, and make a "HashA" when it is big enough `pub enum Either { A(Box<Either>), B, HashA(Box<LazyHash<Either>>) }`. it is possible but not operatable for me.

Overall, I feel salsa's intern-based solution is more efficient for comparing and computing many small inputs than comemo's hash-based solution.

> That's a problem indeed. If you have any proposals how we could extend comemo to make eviction smarter, I'd be interested to hear them. Maybe we can steal some stuff from salsa here, e.g. the durability concept.
> Yeah, I think that's a more fundamental problem. Maybe we could build some cancellation feature into comemo. How big is this problem in practice? Do you frequently notice a lack of cancellation or would it just be cleaner to be able to do it?
> I looked at salsa before building comemo. Maybe I just didn't "get it", but it felt a lot more complicated and it also forces you to write the compiler in a very specific way. Do you think there are some things to its approach that are fundamentally impossible in the comemo approach, rather than just not implemented?

I believe you have investigated salsa and there should be wonderful stuff to steal, for example, the durability concept, which also looks matter for optimizing the performance when the incoming events are mostly user editions. I mentioned these Cons of comemo, because I want to let you know which problems I've encountered when I try to cache static analysis with comemo. We can revisit them when they're becoming a big problem during building our LSP/IDE features.



