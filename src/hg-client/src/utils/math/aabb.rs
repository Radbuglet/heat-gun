use std::{iter, ops::Range};

use macroquad::math::{IVec2, Vec2};

use super::{
    glam::{AaLine, AaLineI, Axis2, TileFace},
    CopyRange, Segment, Sign, Vec2Ext,
};

// === AABB === //

#[derive(Debug, Copy, Clone)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub const NAN: Self = Self {
        min: Vec2::NAN,
        max: Vec2::NAN,
    };

    pub const ZERO: Self = Self {
        min: Vec2::ZERO,
        max: Vec2::ZERO,
    };

    pub const ZERO_TO_ONE: Self = Self {
        min: Vec2::ZERO,
        max: Vec2::ONE,
    };

    pub const EVERYWHERE: Self = Self {
        min: Vec2::NEG_INFINITY,
        max: Vec2::INFINITY,
    };

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self::new_sized(Vec2::new(x, y), Vec2::new(w, h))
    }

    pub fn new_poly(poly: &[Vec2]) -> Self {
        let min = poly.iter().copied().reduce(Vec2::min).unwrap();
        let max = poly.iter().copied().reduce(Vec2::max).unwrap();

        Self { min, max }
    }

    pub fn point_at(&self, percent: Vec2) -> Vec2 {
        self.min + self.size() * percent
    }

    pub fn new_centered(center: Vec2, size: Vec2) -> Self {
        let half_size = size / 2.0;
        Self {
            min: center - half_size,
            max: center + half_size,
        }
    }

    pub fn new_sized(min: Vec2, size: Vec2) -> Self {
        Self {
            min,
            max: min + size,
        }
    }

    pub fn translated(self, rel: Vec2) -> Self {
        Self {
            min: self.min + rel,
            max: self.max + rel,
        }
    }

    pub fn translate_extend(self, rel: Vec2) -> Self {
        let target = self.translated(rel);
        Self {
            min: self.min.min(target.min),
            max: self.max.max(target.max),
        }
    }

    pub fn contains(self, point: Vec2) -> bool {
        (self.min.cmple(point) & point.cmple(self.max)).all()
    }

    pub fn intersects(self, other: Self) -> bool {
        !(
            // We're entirely to the left
            self.max.x < other.min.x ||
            // We're entirely to the right
            other.max.x < self.min.x ||
            // We're entirely above
            self.max.y < other.min.y ||
            // We're entirely below
            other.max.y < self.min.y
        )
    }

    pub fn normalized(self) -> Self {
        let min = self.min.min(self.max);
        let max = self.min.max(self.max);
        Self { min, max }
    }

    pub fn clamped(self) -> Self {
        Self {
            min: self.min.min(self.max),
            max: self.max.max(self.min),
        }
    }

    pub fn clamp(self, pos: Vec2) -> Vec2 {
        pos.clamp(self.min, self.max)
    }

    pub fn grow(self, by: Vec2) -> Self {
        Self::new_centered(self.center(), self.size() + by)
    }

    pub fn shrink(self, by: Vec2) -> Self {
        self.grow(-by)
    }

    pub fn center(self) -> Vec2 {
        self.min.lerp(self.max, 0.5)
    }

    pub fn size(self) -> Vec2 {
        self.max - self.min
    }

    pub fn x(self) -> f32 {
        self.min.x
    }

    pub fn y(self) -> f32 {
        self.min.y
    }

    pub fn w(self) -> f32 {
        self.size().x
    }

    pub fn h(self) -> f32 {
        self.size().y
    }

    pub fn is_nan(self) -> bool {
        self.min.is_nan() || self.max.is_nan()
    }

    pub fn corners(self) -> [Vec2; 4] {
        let Vec2 { x: x_min, y: y_min } = self.min;
        let Vec2 { x: x_max, y: y_max } = self.min;

        [
            Vec2::new(x_min, y_min),
            Vec2::new(x_max, y_min),
            Vec2::new(x_max, y_max),
            Vec2::new(x_min, y_max),
        ]
    }

    pub fn corner(self, axis: Axis2, sign: Sign) -> f32 {
        match sign {
            Sign::Pos => self.max,
            Sign::Neg => self.min,
        }
        .axis(axis)
    }

    pub fn edge_line(self, face: TileFace) -> AaLine {
        use TileFace::*;

        match face {
            Left => AaLine {
                axis: Axis2::X,
                norm: self.min.x,
            },
            Right => AaLine {
                axis: Axis2::X,
                norm: self.max.x,
            },
            Top => AaLine {
                axis: Axis2::Y,
                norm: self.min.y,
            },
            Bottom => AaLine {
                axis: Axis2::Y,
                norm: self.max.y,
            },
        }
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn edges(self) -> [Segment; 4] {
        let [a, b, c, d] = self.corners();

        [
            Segment::new_points(a, b),
            Segment::new_points(b, c),
            Segment::new_points(c, d),
            Segment::new_points(d, a),
        ]
    }
}

// === AabbI === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub struct AabbI {
    pub min: IVec2,
    pub max: IVec2,
}

impl AabbI {
    pub const ZERO: AabbI = AabbI {
        min: IVec2::ZERO,
        max: IVec2::ZERO,
    };

    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self::new_sized(IVec2::new(x, y), IVec2::new(w, h))
    }

    pub const fn new_sized(min: IVec2, size: IVec2) -> Self {
        Self {
            min,
            max: IVec2::new(min.x + size.x, min.y + size.y),
        }
    }

    pub fn normalized(self) -> Self {
        let min = self.min.min(self.max);
        let max = self.min.max(self.max);
        Self { min, max }
    }

    pub fn inclusive(self) -> Self {
        Self {
            min: self.min,
            max: self.max + IVec2::ONE,
        }
    }

    pub fn iter_exclusive(mut self) -> impl Iterator<Item = IVec2> {
        self = self.normalized();

        let mut pos = self.min - IVec2::X;
        iter::from_fn(move || {
            if pos.x < self.max.x {
                pos.x += 1;
            } else {
                pos.x = self.min.x;
                pos.y += 1;
            }

            (pos.y < self.max.y).then_some(pos)
        })
    }

    pub fn iter_inclusive(self) -> impl Iterator<Item = IVec2> {
        self.inclusive().iter_exclusive()
    }

    pub fn size(self) -> IVec2 {
        self.max - self.min
    }

    pub fn as_aabb(self) -> Aabb {
        Aabb {
            min: self.min.as_vec2(),
            max: self.max.as_vec2(),
        }
    }

    pub fn edge_line(self, face: TileFace) -> AaLineI {
        use TileFace::*;

        match face {
            Left => AaLineI {
                axis: Axis2::X,
                norm: self.min.x,
            },
            Right => AaLineI {
                axis: Axis2::X,
                norm: self.max.x,
            },
            Top => AaLineI {
                axis: Axis2::Y,
                norm: self.min.y,
            },
            Bottom => AaLineI {
                axis: Axis2::Y,
                norm: self.max.y,
            },
        }
    }

    pub fn x_range(self) -> Range<i32> {
        self.min.x..self.max.x
    }

    pub fn y_range(self) -> Range<i32> {
        self.min.y..self.max.y
    }

    pub fn diff_exclusive(self, without: AabbI) -> impl Iterator<Item = IVec2> {
        let y_diff = RangeDiff::of(self.y_range(), without.y_range());

        let full_rows = y_diff
            .included()
            .flat_map(move |y| self.x_range().into_iter().map(move |x| IVec2::new(x, y)));

        let partial_x_diff = RangeDiff::of(self.x_range(), without.x_range());
        let partial_rows = y_diff
            .excluded
            .into_range()
            .flat_map(move |y| partial_x_diff.included().map(move |x| IVec2::new(x, y)));

        full_rows.chain(partial_rows)
    }

    pub fn diff_inclusive(self, without: AabbI) -> impl Iterator<Item = IVec2> {
        self.inclusive().diff_exclusive(without.inclusive())
    }
}

#[derive(Debug, Copy, Clone)]
struct RangeDiff {
    excluded: CopyRange<i32>,
    included: [CopyRange<i32>; 2],
}

impl RangeDiff {
    fn of(with: Range<i32>, without: Range<i32>) -> Self {
        let excluded = CopyRange::new(without.start.max(with.start)..without.end.min(with.end));
        let included = [with.start..excluded.start, excluded.end..with.end].map(CopyRange::new);

        Self { excluded, included }
    }

    fn included(self) -> impl Iterator<Item = i32> {
        self.included.into_iter().flat_map(|v| v.into_range())
    }
}
