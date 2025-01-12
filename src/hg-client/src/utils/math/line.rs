use macroquad::math::{Mat2, Vec2};

// === Segment === //

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Segment {
    pub start: Vec2,
    pub end: Vec2,
}

impl Segment {
    pub fn new_delta(start: Vec2, delta: Vec2) -> Self {
        Self {
            start,
            end: start + delta,
        }
    }

    pub fn new_points(start: Vec2, end: Vec2) -> Self {
        Self { start, end }
    }

    pub fn swap(self) -> Self {
        Self {
            start: self.end,
            end: self.start,
        }
    }

    pub fn delta(self) -> Vec2 {
        self.end - self.start
    }

    pub fn len(self) -> f32 {
        (self.end - self.start).length()
    }

    pub fn lerp(self, v: f32) -> Vec2 {
        self.start.lerp(self.end, v)
    }

    pub fn intersect_raw(self, other: Segment) -> IntersectResult {
        // We can describe our two segments as functions `L₁,L₂: ℝ → ℝ²` defined as...
        //
        //   L₁(t) = S₁ + (E₁ - S₁) * t     L₂(t) = S₂ + (E₂ - S₂) * t
        //
        // ...where each Sₙ denotes a start point and Eₙ denotes an end point.
        //
        // These line segments intersect when there exists t₁,t₂: [0, 1] such that...
        //
        //   L₂(t₂) = L₁(t₁)
        //
        // With a bit of algebra, we can get to...
        //
        //   S₂ + (E₂ - S₂) * t₂ = S₁ + (E₁ - S₁) * t₁
        //   S₂ - S₁ + (E₂ - S₂) * t₂ = (E₁ - S₁) * t₁
        //   S₂ - S₁ = (E₁ - S₁) * t₁ - (E₂ - S₂) * t₂
        //   S₂ - S₁ = (E₁ - S₁) * t₁ + (S₂ - E₂) * t₂
        //
        // Reformulating using matrices, we find that...
        //
        //   ╔    ┆         ┆    ╗ ╔    ╗
        //   ║    ┆         ┆    ║ ║ t₁ ║
        //   ║ E₁ - S₁   S₂ - E₂ ║ ║    ║  =  S₂ - S₁
        //   ║    ┆         ┆    ║ ║ t₂ ║
        //   ╚    ┆         ┆    ╝ ╚    ╝
        //
        // This gives us a very straightforward way of computing t₁ and t₂.

        let mat = Mat2::from_cols(self.end - self.start, other.start - other.end);

        if mat.determinant() == 0. {
            return IntersectResult::DEGENERATE;
        }

        let Vec2 {
            x: dist_self,
            y: dist_other,
        } = mat.inverse() * (other.start - self.start);

        IntersectResult {
            lerp_self: dist_self,
            lerp_other: dist_other,
        }
    }

    pub fn intersect_ext(
        self,
        other: Segment,
        self_cap: impl IntersectCap,
        other_cap: impl IntersectCap,
    ) -> (Option<Vec2>, IntersectResult) {
        let res = self.intersect_raw(other);
        let pos = res
            .is_valid(self_cap, other_cap)
            .then(|| self.lerp(res.lerp_self));

        (pos, res)
    }

    pub fn intersect(self, other: Segment) -> (Option<Vec2>, IntersectResult) {
        self.intersect_ext(other, SegmentCap, SegmentCap)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct IntersectResult {
    pub lerp_self: f32,
    pub lerp_other: f32,
}

impl IntersectResult {
    pub const DEGENERATE: Self = Self {
        lerp_self: f32::NAN,
        lerp_other: f32::NAN,
    };

    pub fn is_self_valid(self, cap: impl IntersectCap) -> bool {
        cap.in_range(self.lerp_self)
    }

    pub fn is_other_valid(self, cap: impl IntersectCap) -> bool {
        cap.in_range(self.lerp_other)
    }

    pub fn is_valid(self, self_cap: impl IntersectCap, other_cap: impl IntersectCap) -> bool {
        self.is_self_valid(self_cap) && self.is_other_valid(other_cap)
    }

    pub fn is_degenerate(self) -> bool {
        self.lerp_self.is_nan() && self.lerp_other.is_nan()
    }
}

// === IntersectCap === //

pub trait IntersectCap: Copy {
    fn in_range(self, v: f32) -> bool;
}

impl IntersectCap for (bool, bool) {
    fn in_range(self, v: f32) -> bool {
        let (start, end) = self;

        if v.is_nan() {
            return false;
        }

        if start && !(0. <= v) {
            return false;
        }

        if end && !(v <= 1.0) {
            return false;
        }

        true
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct SegmentCap;

impl IntersectCap for SegmentCap {
    fn in_range(self, v: f32) -> bool {
        0. <= v && v <= 1.
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct RangeCap;

impl IntersectCap for RangeCap {
    fn in_range(self, v: f32) -> bool {
        0. <= v
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct LineCap;

impl IntersectCap for LineCap {
    fn in_range(self, v: f32) -> bool {
        !v.is_nan()
    }
}
