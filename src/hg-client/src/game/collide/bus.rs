use std::{
    fmt,
    ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not},
};

use hg_ecs::{component, Entity, Obj, World};
use macroquad::math::Vec2;

use crate::utils::math::Aabb;

// === ColliderBus === //

#[derive(Debug)]
pub struct ColliderBus {}

component!(ColliderBus);

impl ColliderBus {
    pub fn register(mut self: Obj<Self>, collider: Obj<Collider>) {
        todo!()
    }
}

#[derive(Debug)]
pub struct Collider {
    bus: Option<Obj<ColliderBus>>,
    aabb: Aabb,
    mask: ColliderMask,
    material: ColliderMat,
}

impl Collider {
    pub fn aabb(&self) -> Aabb {
        self.aabb
    }

    pub fn set_aabb(&mut self, aabb: Aabb) {
        self.aabb = aabb;

        // TODO: Update bus
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
    pub check_point: fn(&mut World, Entity, Vec2) -> bool,
}

impl fmt::Debug for CustomColliderMat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomColliderMat")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
