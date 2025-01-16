use std::ops;

use macroquad::math::{BVec2, IVec2, Vec2};

use crate::utils::lang::extension::Extends;

// === Glam Extensions === //

pub trait Vec2Ext: Extends<Vec2> {
    fn mask(self, mask: BVec2) -> Self;
    fn mask_in_axis(self, axis: Axis2) -> Self;
    fn mask_out_axis(self, axis: Axis2) -> Self;
    fn axis(self, axis: Axis2) -> f32;
    fn set_axis(&mut self, axis: Axis2, value: f32);
    fn axis_mut(&mut self, axis: Axis2) -> &mut f32;
}

impl Vec2Ext for Vec2 {
    fn mask(self, mask: BVec2) -> Self {
        Self::select(mask, self, Vec2::ZERO)
    }

    fn mask_in_axis(self, axis: Axis2) -> Self {
        self.mask(axis.mask())
    }

    fn mask_out_axis(self, axis: Axis2) -> Self {
        self.mask(!axis.mask())
    }

    fn axis(self, axis: Axis2) -> f32 {
        self[axis as usize]
    }

    fn set_axis(&mut self, axis: Axis2, value: f32) {
        self[axis as usize] = value;
    }

    fn axis_mut(&mut self, axis: Axis2) -> &mut f32 {
        &mut self[axis as usize]
    }
}

// === Axis-Aligned Constructs === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub enum Axis2 {
    X,
    Y,
}

impl Axis2 {
    pub const AXES: [Self; 2] = [Self::X, Self::Y];

    pub fn iter() -> impl Iterator<Item = Self> {
        Self::AXES.into_iter()
    }

    pub fn mask(self) -> BVec2 {
        match self {
            Self::X => BVec2::new(true, false),
            Self::Y => BVec2::new(false, true),
        }
    }

    pub fn unit_mag(self, comp: f32) -> Vec2 {
        match self {
            Self::X => Vec2::new(comp, 0.),
            Self::Y => Vec2::new(0., comp),
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub enum Sign {
    Pos,
    Neg,
}

impl Sign {
    pub fn of_biased(v: f32) -> Self {
        if v < 0. {
            Self::Neg
        } else {
            Self::Pos
        }
    }

    pub fn unit_mag(self, v: f32) -> f32 {
        if self == Sign::Neg {
            -v
        } else {
            v
        }
    }
}

impl ops::Neg for Sign {
    type Output = Self;

    fn neg(self) -> Self::Output {
        match self {
            Self::Pos => Self::Neg,
            Self::Neg => Self::Pos,
        }
    }
}

pub fn add_magnitude(v: f32, by: f32) -> f32 {
    v + Sign::of_biased(v).unit_mag(by)
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum TileFace {
    Left,
    Right,
    Top,
    Bottom,
}

impl TileFace {
    pub fn compose(axis: Axis2, sign: Sign) -> Self {
        match (axis, sign) {
            (Axis2::X, Sign::Neg) => Self::Left,
            (Axis2::X, Sign::Pos) => Self::Right,
            (Axis2::Y, Sign::Neg) => Self::Top,
            (Axis2::Y, Sign::Pos) => Self::Bottom,
        }
    }

    pub fn axis(self) -> Axis2 {
        match self {
            Self::Left | Self::Right => Axis2::X,
            Self::Top | Self::Bottom => Axis2::Y,
        }
    }

    pub fn sign(self) -> Sign {
        match self {
            Self::Left | Self::Top => Sign::Neg,
            Self::Right | Self::Bottom => Sign::Pos,
        }
    }

    pub fn invert(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
        }
    }

    pub fn as_vec(self) -> Vec2 {
        match self {
            Self::Left => Vec2::NEG_X,
            Self::Right => Vec2::X,
            Self::Top => Vec2::NEG_Y,
            Self::Bottom => Vec2::Y,
        }
    }

    pub fn as_ivec(self) -> IVec2 {
        match self {
            Self::Left => IVec2::NEG_X,
            Self::Right => IVec2::X,
            Self::Top => IVec2::NEG_Y,
            Self::Bottom => IVec2::Y,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct AaLineI {
    pub axis: Axis2,
    pub norm: i32,
}

impl AaLineI {
    pub fn as_aaline(self) -> AaLine {
        AaLine {
            axis: self.axis,
            norm: self.norm as f32,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct AaLine {
    pub axis: Axis2,
    pub norm: f32,
}
