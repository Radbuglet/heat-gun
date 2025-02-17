#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum SocketCloseReason {
    Crash = 1,
    Application = 2,
}

impl SocketCloseReason {
    pub const fn code(self) -> u32 {
        self as u32
    }
}
