use std::{
    context::{pack, Bundle},
    fmt,
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, ControlFlow, Not},
};

use hg_ecs::{component, query::query_removed, AccessComp, Entity, Obj, World, WORLD};

use crate::utils::math::{Aabb, Bhv, BvhNodeIdx, HullCastRequest};

// === ColliderBus === //

pub type ColliderLookupCx<'a> = (
    &'a mut WORLD,
    &'a mut AccessComp<ColliderBus>,
    &'a mut AccessComp<Collider>,
);

#[derive(Debug, Default)]
pub struct ColliderBus {
    pub tree: Bhv<Aabb, Obj<Collider>>,
}

component!(ColliderBus);

impl ColliderBus {
    pub fn register(mut self: Obj<Self>, mut collider: Obj<Collider>) {
        assert!(collider.bus.is_none());

        let bhv_idx = self.tree.insert(collider.aabb, collider);
        collider.bus = Some(self);
        collider.bhv_idx = bhv_idx;
    }

    pub fn lookup<B>(
        self: Obj<Self>,
        lookup: Aabb,
        mut predicate: impl FnMut((Obj<Collider>, Bundle<ColliderLookupCx<'_>>)) -> ControlFlow<B>,
    ) -> ControlFlow<B> {
        let mut queue = self.tree.root_idx().into_iter().collect::<Vec<_>>();

        while let Some(curr) = queue.pop() {
            let curr = self.tree.node(curr);

            if !curr.aabb().intersects(lookup) {
                continue;
            }

            let Some(&candidate) = curr.opt_value() else {
                queue.extend(curr.children_idx());
                continue;
            };

            predicate((candidate, pack!(@env)))?;
        }

        ControlFlow::Continue(())
    }

    pub fn check_aabb(
        self: Obj<Self>,
        aabb: Aabb,
        mut predicate: impl FnMut(Obj<Collider>, Bundle<ColliderLookupCx<'_>>) -> bool,
    ) -> bool {
        cbit::cbit!(for (collider, cx) in self.lookup(aabb) {
            let static ..cx;

            if !predicate(collider) {
                continue;
            }

            match collider.material {
                ColliderMat::Solid => return true,
                ColliderMat::Disabled => {
                    // (cannot collide)
                }
                ColliderMat::Custom(mat) => {
                    let entity = collider.entity();
                    if (mat.check_aabb)(&mut WORLD, entity, aabb) {
                        return true;
                    }
                }
            }
        });

        false
    }

    pub fn check_hull_percent(
        self: Obj<Self>,
        request: HullCastRequest,
        mut predicate: impl FnMut(Obj<Collider>, Bundle<ColliderLookupCx<'_>>) -> bool,
    ) -> f32 {
        let mut max_trans = 1f32;

        cbit::cbit!(for (collider, cx) in self.lookup(request.candidate_aabb()) {
            let static ..cx;

            if !predicate(collider) {
                continue;
            }

            match collider.material {
                ColliderMat::Solid => {
                    max_trans = max_trans.min(request.hull_cast_percent(collider.aabb));
                }
                ColliderMat::Disabled => {
                    // ignore
                }
                ColliderMat::Custom(mat) => {
                    let entity = collider.entity();
                    let max_trans_local = (mat.check_hull_percent)(&mut WORLD, entity, request);

                    max_trans = max_trans.min(max_trans_local);
                }
            }
        });

        max_trans
    }

    pub fn check_hull(
        self: Obj<Self>,
        request: HullCastRequest,
        predicate: impl FnMut(Obj<Collider>, Bundle<ColliderLookupCx<'_>>) -> bool,
    ) -> f32 {
        self.check_hull_percent(request, predicate) * request.delta_len()
    }
}

// === Collider === //

#[derive(Debug)]
pub struct Collider {
    bus: Option<Obj<ColliderBus>>,
    bhv_idx: BvhNodeIdx,
    aabb: Aabb,
    mask: ColliderMask,
    material: ColliderMat,
}

impl Collider {
    pub fn new(mask: ColliderMask, material: ColliderMat) -> Self {
        Self {
            bus: None,
            bhv_idx: BvhNodeIdx::DANGLING,
            aabb: Aabb::ZERO,
            mask,
            material,
        }
    }

    pub fn unregister(&mut self) {
        if let Some(mut bus) = self.bus.take() {
            bus.tree.remove(self.bhv_idx);
        }
    }

    pub fn aabb(&self) -> Aabb {
        self.aabb
    }

    pub fn set_aabb(&mut self, aabb: Aabb) {
        self.aabb = aabb;

        if let Some(mut bus) = self.bus {
            bus.tree.update_aabb(self.bhv_idx, aabb);
        }
    }

    pub fn mask(&self) -> ColliderMask {
        self.mask
    }

    pub fn set_mask(&mut self, mask: ColliderMask) {
        self.mask = mask;
    }

    pub fn material(&self) -> ColliderMat {
        self.material
    }

    pub fn set_material(&mut self, material: ColliderMat) {
        self.material = material;
    }
}

component!(Collider);

pub fn register_collider(collider: Obj<Collider>) {
    collider
        .entity()
        .deep_get::<ColliderBus>()
        .register(collider);
}

// === ColliderMask === //

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub struct ColliderMask(u64);

impl fmt::Debug for ColliderMask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ColliderMask")
            .field(&format_args!("{:b}", self.0))
            .finish()
    }
}

impl BitOr for ColliderMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for ColliderMask {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

impl BitAnd for ColliderMask {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for ColliderMask {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs;
    }
}

impl BitXor for ColliderMask {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for ColliderMask {
    fn bitxor_assign(&mut self, rhs: Self) {
        *self = *self ^ rhs;
    }
}

impl Not for ColliderMask {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl FromIterator<ColliderMask> for ColliderMask {
    fn from_iter<T: IntoIterator<Item = ColliderMask>>(iter: T) -> Self {
        let mut accum = Self::NONE;

        for item in iter {
            accum |= item;
        }

        accum
    }
}

impl ColliderMask {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(u64::MAX);

    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn to_raw(self) -> u64 {
        self.0
    }

    pub const fn one(at: usize) -> Self {
        Self(1 << at)
    }

    pub fn is_empty(self) -> bool {
        self == Self::NONE
    }

    pub fn intersects(self, other: Self) -> bool {
        !(self & other).is_empty()
    }
}

// === ColliderMat === //

#[derive(Debug, Copy, Clone)]
pub enum ColliderMat {
    Solid,
    Disabled,
    Custom(&'static CustomColliderMat),
}

pub struct CustomColliderMat {
    pub name: &'static str,
    pub check_aabb: fn(&mut World, Entity, Aabb) -> bool,
    pub check_hull_percent: fn(&mut World, Entity, HullCastRequest) -> f32,
}

impl fmt::Debug for CustomColliderMat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomColliderMat")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

// === Systems === //

pub fn sys_flush_colliders() {
    for mut collider in query_removed::<Collider>() {
        collider.unregister();
    }
}
