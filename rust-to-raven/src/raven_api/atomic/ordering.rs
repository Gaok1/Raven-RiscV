#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordering {
    Relaxed,
    Release,
    Acquire,
    AcqRel,
    SeqCst,
}

impl Ordering {
    #[inline(always)]
    pub(crate) const fn needs_acquire_fence(self) -> bool {
        matches!(self, Self::Acquire | Self::AcqRel | Self::SeqCst)
    }

    #[inline(always)]
    pub(crate) const fn needs_release_fence(self) -> bool {
        matches!(self, Self::Release | Self::AcqRel | Self::SeqCst)
    }
}
