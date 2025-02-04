use std::{
    context::{infer_bundle, pack, Bundle},
    fmt, iter,
    rc::Rc,
};

use hg_ecs::{archetype::ComponentId, component, entity::Component, AccessComp, Entity, WORLD};
use hg_utils::hash::{hash_map::Entry, hash_set, FxHashMap, FxHashSet};

// === Graphics Bus === //

pub fn register_gfx(target: Entity) {
    assert!(
        target.try_get::<GfxParticipant>().is_none(),
        "registered {target:?} as a graphics object more than once"
    );

    let mut iter = Some(target);
    let arch_store = Entity::archetypes();
    let arch = target.archetype();

    while let Some(curr) = iter {
        iter = curr.parent();

        let Some(mut node) = curr.try_get::<GraphicsNode>() else {
            continue;
        };

        for comp in arch_store.components(arch) {
            let Some(list) = node.descendants.get_mut(comp) else {
                continue;
            };

            list.mutate().insert(target);
        }
    }

    target.add(GfxParticipant);
}

fn find_gfx_inner<'a>(
    ancestor: Entity,
    id: ComponentId,
    cx: Bundle<(&'a WORLD, &'a mut AccessComp<GraphicsNode>)>,
) -> &'a mut GfxNodeCollection {
    let static ..cx;

    let mut node = match ancestor.try_get::<GraphicsNode>() {
        Some(parent) => parent,
        None => ancestor.add(GraphicsNode::default()),
    };

    let entry = match node.descendants.entry(id) {
        Entry::Occupied(entry) => {
            return entry.into_mut();
        }
        Entry::Vacant(entry) => entry,
    };

    let mut descendants = FxHashSet::default();

    let mut visit_stack = vec![ancestor];

    while let Some(target) = visit_stack.pop() {
        visit_stack.extend(&target.children());

        let comps = Entity::archetypes().components_set(target.archetype());

        if comps.contains(&id) && comps.contains(&ComponentId::of::<GfxParticipant>()) {
            descendants.insert(target);
        }
    }

    let descendants = GfxNodeCollection {
        nodes: Rc::new(descendants),
    };
    entry.insert(descendants.clone())
}

pub fn find_gfx_with(ancestor: Entity, id: ComponentId) -> GfxNodeCollection {
    let collection = find_gfx_inner(ancestor, id);

    if let Some(collection) = collection.try_mutate() {
        let cx = pack!(@env => Bundle<infer_bundle!('_)>);
        collection.retain(|v| {
            let static ..cx;
            v.is_alive()
        });
    }

    collection.clone()
}

pub fn find_gfx<T: Component>(ancestor: Entity) -> GfxNodeCollection {
    find_gfx_with(ancestor, ComponentId::of::<T>())
}

#[derive(Clone)]
pub struct GfxNodeCollection {
    nodes: Rc<FxHashSet<Entity>>,
}

impl fmt::Debug for GfxNodeCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.nodes.iter()).finish()
    }
}

impl GfxNodeCollection {
    fn try_mutate(&mut self) -> Option<&mut FxHashSet<Entity>> {
        Rc::get_mut(&mut self.nodes)
    }

    fn mutate(&mut self) -> &mut FxHashSet<Entity> {
        self.try_mutate()
            .expect("cannot mutate a `GfxNodeCollection` while it's being iterated over")
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl<'a> IntoIterator for &'a GfxNodeCollection {
    type Item = Entity;
    type IntoIter = iter::Copied<hash_set::Iter<'a, Entity>>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.iter().copied()
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct GfxParticipant;

component!(GfxParticipant);

#[derive(Default)]
pub struct GraphicsNode {
    descendants: FxHashMap<ComponentId, GfxNodeCollection>,
}

component!(GraphicsNode);

impl fmt::Debug for GraphicsNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphicsNode").finish_non_exhaustive()
    }
}
