use macroquad::math::{BVec2, IVec2, Vec2};

// === Glam Extensions === //

pub trait Vec2Ext: Extends<Vec2> {
    fn mask(self, mask: BVec2) -> Self;
    fn mask_in_axis(self, axis: Axis2) -> Self;
    fn mask_out_axis(self, axis: Axis2) -> Self;
    fn get_axis(self, axis: Axis2) -> f32;
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

    fn get_axis(self, axis: Axis2) -> f32 {
        self[axis as usize]
    }
}

// === Axis-Aligned Constructs === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub enum Axis2 {
    X,
    Y,
}

use Axis2::*;

impl Axis2 {
    pub const AXES: [Self; 2] = [Self::X, Self::Y];

    pub fn iter() -> impl Iterator<Item = Self> {
        Self::AXES.into_iter()
    }

    pub fn mask(self) -> BVec2 {
        match self {
            X => BVec2::new(true, false),
            Y => BVec2::new(false, true),
        }
    }

    pub fn unit_mag(self, comp: f32) -> Vec2 {
        match self {
            X => Vec2::new(comp, 0.),
            Y => Vec2::new(0., comp),
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub enum Sign {
    Pos,
    Neg,
}

use Sign::*;

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

use TileFace::*;

use crate::utils::lang::extension::Extends;

impl TileFace {
    pub fn compose(axis: Axis2, sign: Sign) -> Self {
        match (axis, sign) {
            (X, Neg) => Left,
            (X, Pos) => Right,
            (Y, Neg) => Top,
            (Y, Pos) => Bottom,
        }
    }

    pub fn axis(self) -> Axis2 {
        match self {
            Left | Right => X,
            Top | Bottom => Y,
        }
    }

    pub fn sign(self) -> Sign {
        match self {
            Left | Top => Neg,
            Right | Bottom => Pos,
        }
    }

    pub fn invert(self) -> Self {
        match self {
            Left => Right,
            Right => Left,
            Top => Bottom,
            Bottom => Top,
        }
    }

    pub fn as_vec(self) -> Vec2 {
        match self {
            Left => Vec2::NEG_X,
            Right => Vec2::X,
            Top => Vec2::NEG_Y,
            Bottom => Vec2::Y,
        }
    }

    pub fn as_ivec(self) -> IVec2 {
        match self {
            Left => IVec2::NEG_X,
            Right => IVec2::X,
            Top => IVec2::NEG_Y,
            Bottom => IVec2::Y,
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
