//! An implementation of [Dynamic Bounding Volume Hierarchies](bvh-gdc).
//!
//! [bvh-gdc]: https://box2d.org/files/ErinCatto_DynamicBVH_GDC2019.pdf

use std::{cmp, collections::BinaryHeap, fmt};

use derive_where::derive_where;
use smallvec::SmallVec;
use thunderdome::{Arena, Index};

use super::Aabb;

// === GenericAabb === //

pub trait GenericAabb: Copy {
    fn surface_area(self) -> f32;

    fn union(self, other: Self) -> Self;
}

impl GenericAabb for Aabb {
    fn surface_area(self) -> f32 {
        (self.w() + self.h()) * 2.
    }

    fn union(self, other: Self) -> Self {
        self.union(other)
    }
}

// === BhvTree === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct BvhNodeIdx(Index);

impl BvhNodeIdx {
    pub const DANGLING: Self = Self(Index::DANGLING);
}

pub struct Bhv<A, T> {
    nodes: Arena<Node<A, T>>,
    root: Option<BvhNodeIdx>,
}

struct Node<A, T> {
    aabb: A,
    parent: Option<BvhNodeIdx>,
    kind: NodeKind<T>,
}

enum NodeKind<T> {
    Branch { children: [BvhNodeIdx; 2] },
    Leaf { value: T },
}

impl<A, T> fmt::Debug for Bhv<A, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AabbTree").finish_non_exhaustive()
    }
}

impl<A, T> Bhv<A, T> {
    pub const fn new() -> Self {
        Self {
            nodes: Arena::new(),
            root: None,
        }
    }

    pub fn opt_node(&self, idx: BvhNodeIdx) -> Option<BvhNodeView<'_, A, T>> {
        self.nodes.get(idx.0).map(|node| BvhNodeView {
            tree: self,
            node,
            idx,
        })
    }

    pub fn node(&self, idx: BvhNodeIdx) -> BvhNodeView<'_, A, T> {
        self.opt_node(idx).expect("node does not exist")
    }

    pub fn root_idx(&self) -> Option<BvhNodeIdx> {
        self.root
    }

    pub fn root(&self) -> Option<BvhNodeView<'_, A, T>> {
        self.root_idx().map(|idx| self.node(idx))
    }
}

impl<A, T> Default for Bhv<A, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A, T> Bhv<A, T>
where
    A: GenericAabb,
{
    fn children_mut(&mut self, node: BvhNodeIdx) -> &mut [BvhNodeIdx; 2] {
        match &mut self.nodes[node.0].kind {
            NodeKind::Branch { children } => children,
            NodeKind::Leaf { .. } => unreachable!(),
        }
    }

    pub fn insert(&mut self, aabb: A, value: T) -> BvhNodeIdx {
        let leaf = BvhNodeIdx(self.nodes.insert(Node {
            aabb,
            parent: None,
            kind: NodeKind::Leaf { value },
        }));

        self.insert_leaf(leaf);
        leaf
    }

    pub fn remove(&mut self, leaf: BvhNodeIdx) -> T {
        assert!(
            self.node(leaf).is_leaf(),
            "attempted to remove a non-leaf node"
        );

        // Orphan simply removes the node from the tree without freeing it.
        self.orphan_leaf(leaf);

        // We have to remove the node and fetch its value ourselves.
        match self.nodes.remove(leaf.0) {
            Some(Node {
                kind: NodeKind::Leaf { value },
                ..
            }) => value,
            _ => unreachable!(),
        }
    }

    pub fn update_aabb(&mut self, leaf: BvhNodeIdx, new_aabb: A) {
        assert!(
            self.node(leaf).is_leaf(),
            "attempted to update the AABB of a non-leaf node"
        );

        // The talk suggests re-insertion so let's use that!
        self.orphan_leaf(leaf);
        self.nodes[leaf.0].aabb = new_aabb;
        self.insert_leaf(leaf);
    }

    fn insert_leaf(&mut self, leaf: BvhNodeIdx) {
        debug_assert!(self.node(leaf).is_leaf());

        if self.root.is_none() {
            self.root = Some(leaf);
        }

        // Stage 1: find the best sibling for the new leaf
        let leaf_aabb = self.nodes[leaf.0].aabb;
        let best_sibling = self.find_best_sibling(leaf_aabb);

        // Stage 2: create the node `new_parent` to hold `best_sibling` and `leaf` together and
        // attach `new_parent` to `old_parent`.
        let old_parent = self.nodes[best_sibling.0].parent;

        let new_aabb = leaf_aabb.union(self.nodes[best_sibling.0].aabb);

        // `old_parent` <- `new_parent` -> `children`
        let new_parent = BvhNodeIdx(self.nodes.insert(Node {
            aabb: new_aabb,
            parent: old_parent,
            kind: NodeKind::Branch {
                children: [best_sibling, leaf],
            },
        }));

        // `old_parent` -> `new_parent`
        if let Some(old_parent) = old_parent {
            let NodeKind::Branch { children } = &mut self.nodes[old_parent.0].kind else {
                unreachable!()
            };

            let old_parent_child = children
                .iter_mut()
                .find(|&&mut v| v == best_sibling)
                .unwrap();

            *old_parent_child = new_parent;
        } else {
            self.root = Some(new_parent);
        }

        // `new_parent` <- `children`
        self.nodes[best_sibling.0].parent = Some(new_parent);
        self.nodes[leaf.0].parent = Some(new_parent);

        // Stage 3: walk back up the tree refitting AABBs and applying rotations

        // We start the iteration at the parent of `leaf` because this routine only operates on
        // branch nodes.
        let mut iter = Some(new_parent);

        while let Some(curr) = iter {
            let Node {
                parent,
                kind: NodeKind::Branch { children },
                ..
            } = self.nodes[curr.0]
            else {
                unreachable!()
            };

            // Refit the ancestor
            let [left_aabb, right_aabb] = children.map(|idx| self.nodes[idx.0].aabb);
            self.nodes[curr.0].aabb = left_aabb.union(right_aabb);

            // Rotate the ancestor to optimize graph cost
            self.rotate_branch_optimally(curr);

            // Proceed to parent
            iter = parent;
        }
    }

    fn find_best_sibling(&self, aabb: A) -> BvhNodeIdx {
        // The total cost of a given tree is defined as the sum of the surface areas of all non-root
        // branch nodes. We ignore leaf nodes and root nodes because their cost does not change based
        // on the organization of a given set of leaf AABBs into a tree.
        //
        // The cost of selecting a given node `S` as a sibling of `N` is given by...
        //
        // dCost = SA(C & S) + dSA(S.parent) + dSA(S.parent.parent) + ...
        //
        // ...where:
        //
        // dSA(X) = SA(X & N) - SA(N)
        //
        // Why? Consider the following portion of a graph and how it changes as we attach a sibling
        // to node `S`...
        //
        //    R                                                 R   <---------- no cost because root
        //   / \                                               / \
        // ...  1   <------- cost is SA(1)             |->   ...  1   <------- cost is SA(1 & N)
        //     / \                                     |          / \
        //   ...  2   <----- cost is SA(2)   ======>   |---->   ...  2   <----- cost is SA(2 & N)
        //       / \                                   |            / \
        //     ...  S   <--- sibling                   |------>   ...  P   <--- cost is SA(S & N)
        //         / \                                 |              / \
        //         ...                                 |--------->   S   N <--- no cost because leaf
        //                                             |            / \
        //                                             |--------->  ...
        //                                             |
        //                                             ---- AABB unchanged
        //
        // Notice that the only portions of the total graph sum to change are...
        //
        // - The strict ancestors `R`, `1`, `2`, whose cost increase by
        //   `SA(X & N) - SA(N) = dSA(X)`.
        //
        // - The introduction of `P`, which has cost `SA(S & N)`
        //
        // For this routine, we want to find a choice of `S` which minimizes this increase in tree
        // cost.

        // Let us define our `sa` and `dsa` functions.
        let sa = |idx: BvhNodeIdx| self.nodes[idx.0].aabb.union(aabb).surface_area();
        let dsa = |idx: BvhNodeIdx| sa(idx) - self.nodes[idx.0].aabb.surface_area();

        // This is a surprise tool to help us later.
        let aabb_sa = aabb.surface_area();

        // We're going to implement this routine by iteratively exploring candidate nodes from most
        // to least promising and only queuing up children of those candidates as new candidates if
        // they could yield a cost lower than the best one we found.
        let mut best_node = BvhNodeIdx::DANGLING;
        let mut best_cost = f32::INFINITY;

        let mut queue = BinaryHeap::new();

        let root = self.root.unwrap();

        queue.push(FbsCandidate {
            node: root,

            // The cost of selecting this node is certainly greater than or equal to zero.
            min_cost: 0.,

            // `inherited_cost_delta` denotes the delta in cost of `node` and its non-root ancestors
            // as we refit them for the supplied `aabb`.
            //
            // In the `SA(C & S) + dSA(S.parent) + dSA(S.parent.parent) + ...`, formula...
            //         ~~~~~~~~~   ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
            //         ^ this is the direct cost...                         ^
            //                              ...and this is the indirect cost.
            //
            // The root node has no ancestors so its inherited cost is zero.
            inherited_cost: 0.,
        });

        while let Some(candidate) = queue.pop() {
            // First, let us determine the cost of choosing `candidate`.
            let candidate_cost = sa(candidate.node) + candidate.inherited_cost;

            // If it's better than our best cost, update it!
            if candidate_cost < best_cost {
                best_node = candidate.node;
                best_cost = candidate_cost;
            }

            // If this node is a branch node...
            let NodeKind::Branch { children } = self.nodes[candidate.node.0].kind else {
                continue;
            };

            // Let us compute the `inherited_cost` for any children we choose to explore.
            let inherited_cost = if candidate.node == root {
                // The root doesn't count towards the cost. It wouldn't be incorrect to leave this
                // out but it would be inconsistent with the rules we defined so we'll keep the
                // check to avoid confusion.
                0.
            } else {
                // We're a non-root parent of our child candidate so include dSA(candidate) in the
                // inherited cost delta sum.
                candidate.inherited_cost + dsa(candidate.node)
            };

            // We know that the cost of all descendants of `candidate` must include `inherited_cost`
            // in their sum. Additionally, we know that SA(X & N) >= SA(N). Hence, the minimum cost
            // of any descendants of this node is...
            //
            // SA(aabb) + inherited_cost
            //
            // If this quantity is higher than the `best_cost`, we know that we'll never find a
            // cheaper node by exploring this sub-tree and can therefore ignore it.
            let min_cost = aabb_sa + inherited_cost;
            if min_cost > best_cost {
                continue;
            }

            // Otherwise, we still have some child candidates to explore.
            for child in children {
                queue.push(FbsCandidate {
                    node: child,
                    min_cost,
                    inherited_cost,
                });
            }
        }

        best_node
    }

    fn rotate_branch_optimally(&mut self, branch: BvhNodeIdx) {
        // This routine attempts to maintain rough tree balance by performing tree rotations on the
        // ancestors of an inserted node that reduce the cost of the tree.
        //
        // A tree rotation, in the context of this routine, is a swap between one grandchild of
        // `node` in one branch and the child of `node` from the opposite branch.

        // The `insert_leaf` routine ensures that it's only passing branch nodes to us.
        let NodeKind::Branch { children } = self.nodes[branch.0].kind else {
            unreachable!()
        };

        // Now, we want to find the best rotation we could apply to `branch`. We do this by seeing
        // how a rotation against a given grandchild of `branch` would modify our tree's total cost
        // and choosing the one which results in the largest *decrease* in this cost.
        //
        // Each element of this array indicates the delta in cost to perform the swap against the
        // i-th grandchild (with the child `node` from the opposite branch being implied). For
        // example, the element `cost_diffs[1][0]` would refer to the cost delta of swapping B and F
        // in the following tree:
        //
        //        A                            A
        //       / \                          / \
        //      /   \                        /   \
        //     /     \        =====>       (F)    C
        //   (B)      C                          / \
        //   / \     / \                       (B)  G
        //  D   E  (F)  G                      / \
        //                                    D   E
        //
        // Note that only `C`'s AABB has to change as a result of this operation and that is goes
        // from `SA(F & G)` to `SA(B & G)`.
        //
        // In other words, the cost delta of swapping a given `grandchild` and the opposite child
        // `other` is equal to...
        //
        // SA(other & grandchild.sibling) - SA(grandchild.parent)
        //
        let cost_diffs: [[f32; 2]; 2] = [0, 1].map(|main_idx| {
            // We want to probe a swap of a child of `main` (grandchild of `branch`) with `other`.
            let other_idx = 1 - main_idx;
            let main = children[main_idx];
            let other = children[other_idx];

            // If `main` has no children, there are no swaps we could do on it so make the costs of
            // those operations infinite to indicate their impossibility.
            let NodeKind::Branch {
                children: grandchildren,
            } = self.nodes[main.0].kind
            else {
                return [f32::INFINITY; 2];
            };

            let [left_grandchild, right_grandchild] =
                grandchildren.map(|grandchild| self.nodes[grandchild.0].aabb);

            // Otherwise, compute the grandchild swap cost using the formula we found above.
            let main_sa = self.nodes[main.0].aabb.surface_area();
            let other_aabb = self.nodes[other.0].aabb;

            [
                // This is not a typo. In the formula, we're taking the surface area of other
                // union'ed with the *sibling* of the grandchild.
                other_aabb.union(right_grandchild).surface_area() - main_sa,
                other_aabb.union(left_grandchild).surface_area() - main_sa,
            ]
        });

        let (best_grandchild, best_cost) = cost_diffs
            .as_flattened()
            .iter()
            .copied()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .unwrap();

        if best_cost > 0. {
            // No point in doing a rotation that increases our cost!
            return;
        }

        // If there's a beneficial rotation that we could perform, let's perform it!
        let (main_idx, grandchild_idx) = match best_grandchild {
            0 => (0, 0),
            1 => (0, 1),
            2 => (1, 0),
            3 => (1, 1),
            _ => unreachable!(),
        };

        let other_idx = 1 - main_idx;

        let main = children[main_idx];
        let other = children[other_idx];
        let grandchild = self.node(main).branch_children_idx()[grandchild_idx];

        // Here's the swap we're trying to do...
        //
        // - `branch`                    - `branch` (1: set child at `other_idx` to `grandchild`)
        // |                             |
        // |--- `main`                   |--- `main` (2: set child at `grandchild_idx` to `other`)
        // |  |                          |  |
        // |  |--- `grandchild`          |  |--- `other` (3: updated parent to `main`)
        // |  |                          |  |
        // |  |--- `sibling`             |  |--- `sibling`
        // |                             |
        // |--- `other`                  |--- `grandchild` (4: updated parent to `branch`)
        //
        // Let's do it!

        // Link 1
        self.children_mut(branch)[other_idx] = grandchild;

        // Link 2
        self.children_mut(main)[grandchild_idx] = other;

        // Link 3
        self.nodes[other.0].parent = Some(main);

        // Link 4
        self.nodes[grandchild.0].parent = Some(branch);
    }

    fn orphan_leaf(&mut self, leaf: BvhNodeIdx) {
        debug_assert!(self.node(leaf).is_leaf());

        let Some(parent) = self.nodes[leaf.0].parent else {
            // There's only one node in this graph, `node`, and it's a root. Remove the reference.
            self.root = None;
            return;
        };

        // Leaf removal is simple because we are a leaf node with a parent that has a full set
        // of children. We just have to replace our parent with our sibling.

        // First, let's determine our sibling and our grandparent.
        let Node {
            parent: grandparent,
            kind: NodeKind::Branch { children },
            ..
        } = self.nodes[parent.0]
        else {
            unreachable!();
        };

        let sibling = children.into_iter().find(|&v| v != leaf).unwrap();

        // We can't swap the node data of our `sibling` with our `parent` because our sibling could
        // be a leaf node whose handle we handed out to consumers. Instead, we must kill our parent
        // and reshape the tree such that `sibling` takes the place of our parent.

        // Let's start with patricide since we have to do that unconditionally.
        self.nodes.remove(parent.0);

        // Let's also create the `grandparent` <- `sibling` link before we forget.
        self.nodes[sibling.0].parent = grandparent;

        // Now, let's create the `grandparent` -> `sibling` link.
        if let Some(grandparent) = grandparent {
            let NodeKind::Branch { children } = &mut self.nodes[grandparent.0].kind else {
                unreachable!();
            };

            // Find the `grandparent`'s link to our `parent`...
            let parent_link = children.iter_mut().find(|&&mut v| v == parent).unwrap();

            // ...and update it to point to our `sibling`.
            *parent_link = sibling;
        } else {
            self.root = Some(sibling);
        }

        // Finally, let's kill our `node`'s `parent` link to avoid confusing behavior for the
        // `parent(node)` public method. This is only really needed for the sake of API consumers.
        self.nodes[leaf.0].parent = None;
    }
}

#[derive(Debug, Copy, Clone)]
struct FbsCandidate {
    node: BvhNodeIdx,
    min_cost: f32,
    inherited_cost: f32,
}

impl Eq for FbsCandidate {}

impl PartialEq for FbsCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.min_cost.total_cmp(&other.min_cost).is_eq()
    }
}

impl Ord for FbsCandidate {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.min_cost.total_cmp(&other.min_cost).reverse()
    }
}

impl PartialOrd for FbsCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// === BvhNodeView === //

#[derive_where(Copy, Clone)]
pub struct BvhNodeView<'a, A, T> {
    tree: &'a Bhv<A, T>,
    node: &'a Node<A, T>,
    idx: BvhNodeIdx,
}

impl<'a, A, T> BvhNodeView<'a, A, T> {
    pub fn tree(self) -> &'a Bhv<A, T> {
        self.tree
    }

    pub fn index(self) -> BvhNodeIdx {
        self.idx
    }

    fn make_view(self, idx: BvhNodeIdx) -> BvhNodeView<'a, A, T> {
        self.tree.node(idx)
    }

    pub fn parent_idx(self) -> Option<BvhNodeIdx> {
        self.node.parent
    }

    pub fn parent(self) -> Option<BvhNodeView<'a, A, T>> {
        self.parent_idx().map(|parent| self.make_view(parent))
    }

    pub fn opt_children_idx(self) -> Option<[BvhNodeIdx; 2]> {
        match self.node.kind {
            NodeKind::Branch { children } => Some(children),
            NodeKind::Leaf { .. } => None,
        }
    }

    pub fn branch_children_idx(self) -> [BvhNodeIdx; 2] {
        self.opt_children_idx().expect("node is not a branch")
    }

    pub fn children_idx(self) -> SmallVec<[BvhNodeIdx; 2]> {
        self.opt_children_idx()
            .map_or(SmallVec::new(), |v| SmallVec::from_iter(v))
    }

    pub fn opt_children(self) -> Option<[BvhNodeView<'a, A, T>; 2]> {
        self.opt_children_idx()
            .map(|children| children.map(|child| self.make_view(child)))
    }

    pub fn children(self) -> SmallVec<[BvhNodeView<'a, A, T>; 2]> {
        self.children_idx()
            .into_iter()
            .map(|idx| self.make_view(idx))
            .collect()
    }

    pub fn branch_children(self) -> [BvhNodeView<'a, A, T>; 2] {
        self.branch_children_idx()
            .map(|child| self.make_view(child))
    }

    pub fn opt_value(self) -> Option<&'a T> {
        match &self.node.kind {
            NodeKind::Leaf { value } => Some(value),
            NodeKind::Branch { .. } => None,
        }
    }

    pub fn aabb_ref(self) -> &'a A {
        &self.node.aabb
    }

    pub fn value(self) -> &'a T {
        self.opt_value().expect("node is not a leaf")
    }

    pub fn is_leaf(self) -> bool {
        match self.node.kind {
            NodeKind::Branch { .. } => false,
            NodeKind::Leaf { .. } => true,
        }
    }

    pub fn is_branch(self) -> bool {
        match self.node.kind {
            NodeKind::Branch { .. } => true,
            NodeKind::Leaf { .. } => false,
        }
    }
}

impl<'a, A, T> BvhNodeView<'a, A, T>
where
    A: GenericAabb,
{
    pub fn aabb(self) -> A {
        self.node.aabb
    }
}
