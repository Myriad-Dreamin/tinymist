//! A disjoint-set forest whose unions are represented by fresh roots.

/// Identifies one set created by a [`UnionFind`].
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SetId(usize);

impl SetId {
    /// Returns the arena index for auxiliary data indexed by set identity.
    fn index(self) -> usize {
        self.0
    }
}

/// A disjoint-set forest with payloads on canonical roots.
///
/// Unlike rank-based union, [`UnionFind::merge_into_new`] makes a fresh root
/// and redirects every merged root to it. This preserves the identity of all
/// pre-merge sets while giving the merged set its own identity and payload.
pub(crate) struct UnionFind<T> {
    nodes: Vec<Node<T>>,
}

struct Node<T> {
    parent: SetId,
    value: Option<T>,
}

impl<T> Default for UnionFind<T> {
    fn default() -> Self {
        Self { nodes: Vec::new() }
    }
}

impl<T> UnionFind<T> {
    /// Creates a singleton set and returns its identity.
    pub(crate) fn insert(&mut self, value: T) -> SetId {
        let id = SetId(self.nodes.len());
        self.nodes.push(Node {
            parent: id,
            value: Some(value),
        });
        id
    }

    /// Returns the number of identities allocated in the arena.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the canonical root without changing the forest.
    pub(crate) fn find(&self, mut id: SetId) -> SetId {
        while self.nodes[id.index()].parent != id {
            id = self.nodes[id.index()].parent;
        }
        id
    }

    /// Returns the canonical root and compresses the traversed path.
    pub(crate) fn find_mut(&mut self, mut id: SetId) -> SetId {
        let root = self.find(id);
        while self.nodes[id.index()].parent != id {
            let parent = self.nodes[id.index()].parent;
            self.nodes[id.index()].parent = root;
            id = parent;
        }
        root
    }

    /// Returns the payload of the set containing `id`.
    pub(crate) fn value(&self, id: SetId) -> &T {
        let root = self.find(id);
        self.nodes[root.index()]
            .value
            .as_ref()
            .expect("a canonical union-find root must own its payload")
    }

    /// Returns the mutable payload of the set containing `id`.
    pub(crate) fn value_mut(&mut self, id: SetId) -> &mut T {
        let root = self.find_mut(id);
        self.nodes[root.index()]
            .value
            .as_mut()
            .expect("a canonical union-find root must own its payload")
    }

    /// Merges distinct sets under a fresh root.
    ///
    /// `merge` receives each canonical payload exactly once. Passing identities
    /// from only one set is a no-op and does not call `merge`.
    pub(crate) fn merge_into_new(
        &mut self,
        ids: impl IntoIterator<Item = SetId>,
        merge: impl FnOnce(Vec<T>) -> T,
    ) -> SetId {
        let mut roots: Vec<_> = ids.into_iter().map(|id| self.find_mut(id)).collect();
        roots.sort_unstable();
        roots.dedup();
        assert!(!roots.is_empty(), "cannot merge an empty set collection");

        if roots.len() == 1 {
            return roots[0];
        }

        let values = roots
            .iter()
            .map(|root| {
                self.nodes[root.index()]
                    .value
                    .take()
                    .expect("a canonical union-find root must own its payload")
            })
            .collect();
        let fresh = self.insert(merge(values));
        for root in roots {
            self.nodes[root.index()].parent = fresh;
        }
        fresh
    }

    #[cfg(test)]
    fn parent(&self, id: SetId) -> SetId {
        self.nodes[id.index()].parent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_redirects_roots_to_a_fresh_payload_owner() {
        let mut sets = UnionFind::default();
        let a = sets.insert(vec!["a"]);
        let b = sets.insert(vec!["b"]);

        let merged = sets.merge_into_new([a, b], |groups| groups.into_iter().flatten().collect());

        assert_ne!(merged, a);
        assert_ne!(merged, b);
        assert_eq!(sets.parent(a), merged);
        assert_eq!(sets.parent(b), merged);
        assert_eq!(sets.find(a), merged);
        assert_eq!(sets.find(b), merged);
        assert_eq!(sets.value(merged), &["a", "b"]);
    }

    #[test]
    fn merging_duplicate_identities_is_a_noop() {
        let mut sets = UnionFind::default();
        let a = sets.insert(42);

        let root = sets.merge_into_new([a, a], |_| panic!("one set must not be merged"));

        assert_eq!(root, a);
        assert_eq!(*sets.value(a), 42);
        assert_eq!(sets.len(), 1);
    }
}
