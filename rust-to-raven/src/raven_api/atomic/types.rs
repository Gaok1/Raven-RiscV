use core::arch::asm;
use core::cell::UnsafeCell;
use core::ops::Deref;
use core::ptr::NonNull;

use super::Ordering;
extern crate alloc;

#[inline(always)]
fn fence_before(ordering: Ordering) {
    if ordering.needs_release_fence() {
        unsafe {
            asm!("fence rw, w", options(nostack, preserves_flags));
        }
    }
}

#[inline(always)]
fn fence_after(ordering: Ordering) {
    if ordering.needs_acquire_fence() {
        unsafe {
            asm!("fence r, rw", options(nostack, preserves_flags));
        }
    }
}

#[inline(always)]
fn raw_load_u32(ptr: *const u32, ordering: Ordering) -> u32 {
    fence_before(ordering);
    let value: u32;
    unsafe {
        asm!(
            "amoadd.w {value}, zero, ({ptr})",
            ptr = in(reg) ptr,
            value = lateout(reg) value,
            options(nostack, preserves_flags),
        );
    }
    fence_after(ordering);
    value
}

#[inline(always)]
fn raw_store_u32(ptr: *mut u32, value: u32, ordering: Ordering) {
    fence_before(ordering);
    unsafe {
        asm!(
            "amoswap.w zero, {value}, ({ptr})",
            ptr = in(reg) ptr,
            value = in(reg) value,
            options(nostack, preserves_flags),
        );
    }
    fence_after(ordering);
}

#[inline(always)]
fn raw_swap_u32(ptr: *mut u32, value: u32, ordering: Ordering) -> u32 {
    fence_before(ordering);
    let previous: u32;
    unsafe {
        asm!(
            "amoswap.w {previous}, {value}, ({ptr})",
            ptr = in(reg) ptr,
            value = in(reg) value,
            previous = lateout(reg) previous,
            options(nostack, preserves_flags),
        );
    }
    fence_after(ordering);
    previous
}

#[inline(always)]
fn raw_fetch_add_u32(ptr: *mut u32, value: u32, ordering: Ordering) -> u32 {
    fence_before(ordering);
    let previous: u32;
    unsafe {
        asm!(
            "amoadd.w {previous}, {value}, ({ptr})",
            ptr = in(reg) ptr,
            value = in(reg) value,
            previous = lateout(reg) previous,
            options(nostack, preserves_flags),
        );
    }
    fence_after(ordering);
    previous
}

#[inline(always)]
fn raw_fetch_and_u32(ptr: *mut u32, value: u32, ordering: Ordering) -> u32 {
    fence_before(ordering);
    let previous: u32;
    unsafe {
        asm!(
            "amoand.w {previous}, {value}, ({ptr})",
            ptr = in(reg) ptr,
            value = in(reg) value,
            previous = lateout(reg) previous,
            options(nostack, preserves_flags),
        );
    }
    fence_after(ordering);
    previous
}

#[inline(always)]
fn raw_fetch_or_u32(ptr: *mut u32, value: u32, ordering: Ordering) -> u32 {
    fence_before(ordering);
    let previous: u32;
    unsafe {
        asm!(
            "amoor.w {previous}, {value}, ({ptr})",
            ptr = in(reg) ptr,
            value = in(reg) value,
            previous = lateout(reg) previous,
            options(nostack, preserves_flags),
        );
    }
    fence_after(ordering);
    previous
}

#[inline(always)]
fn raw_fetch_xor_u32(ptr: *mut u32, value: u32, ordering: Ordering) -> u32 {
    fence_before(ordering);
    let previous: u32;
    unsafe {
        asm!(
            "amoxor.w {previous}, {value}, ({ptr})",
            ptr = in(reg) ptr,
            value = in(reg) value,
            previous = lateout(reg) previous,
            options(nostack, preserves_flags),
        );
    }
    fence_after(ordering);
    previous
}

#[inline(always)]
fn raw_compare_exchange_u32(
    ptr: *mut u32,
    current: u32,
    new: u32,
    success: Ordering,
    failure: Ordering,
) -> Result<u32, u32> {
    fence_before(success);
    loop {
        let loaded: u32;
        let status: u32;
        unsafe {
            asm!(
                "0:",
                "lr.w {loaded}, ({ptr})",
                "bne {loaded}, {current}, 1f",
                "sc.w {status}, {new}, ({ptr})",
                "bnez {status}, 0b",
                "j 2f",
                "1:",
                "li {status}, 1",
                "2:",
                ptr = in(reg) ptr,
                current = in(reg) current,
                new = in(reg) new,
                loaded = lateout(reg) loaded,
                status = lateout(reg) status,
                options(nostack, preserves_flags),
            );
        }

        if loaded != current {
            fence_after(failure);
            return Err(loaded);
        }

        if status == 0 {
            fence_after(success);
            return Ok(loaded);
        }
    }
}

pub struct AtomicU32 {
    value: UnsafeCell<u32>,
}

unsafe impl Sync for AtomicU32 {}
unsafe impl Send for AtomicU32 {}

impl AtomicU32 {
    #[inline(always)]
    pub const fn new(value: u32) -> Self {
        Self {
            value: UnsafeCell::new(value),
        }
    }

    #[inline(always)]
    fn ptr(&self) -> *mut u32 {
        self.value.get()
    }

    #[inline(always)]
    pub fn load(&self, ordering: Ordering) -> u32 {
        raw_load_u32(self.ptr().cast_const(), ordering)
    }

    #[inline(always)]
    pub fn store(&self, value: u32, ordering: Ordering) {
        raw_store_u32(self.ptr(), value, ordering);
    }

    #[inline(always)]
    pub fn swap(&self, value: u32, ordering: Ordering) -> u32 {
        raw_swap_u32(self.ptr(), value, ordering)
    }

    #[inline(always)]
    pub fn compare_exchange(
        &self,
        current: u32,
        new: u32,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u32, u32> {
        raw_compare_exchange_u32(self.ptr(), current, new, success, failure)
    }

    #[inline(always)]
    pub fn compare_exchange_weak(
        &self,
        current: u32,
        new: u32,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u32, u32> {
        raw_compare_exchange_u32(self.ptr(), current, new, success, failure)
    }

    #[inline(always)]
    pub fn fetch_add(&self, value: u32, ordering: Ordering) -> u32 {
        raw_fetch_add_u32(self.ptr(), value, ordering)
    }

    #[inline(always)]
    pub fn fetch_sub(&self, value: u32, ordering: Ordering) -> u32 {
        raw_fetch_add_u32(self.ptr(), value.wrapping_neg(), ordering)
    }

    #[inline(always)]
    pub fn fetch_and(&self, value: u32, ordering: Ordering) -> u32 {
        raw_fetch_and_u32(self.ptr(), value, ordering)
    }

    #[inline(always)]
    pub fn fetch_or(&self, value: u32, ordering: Ordering) -> u32 {
        raw_fetch_or_u32(self.ptr(), value, ordering)
    }

    #[inline(always)]
    pub fn fetch_xor(&self, value: u32, ordering: Ordering) -> u32 {
        raw_fetch_xor_u32(self.ptr(), value, ordering)
    }

    pub fn fetch_update<F>(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        mut f: F,
    ) -> Result<u32, u32>
    where
        F: FnMut(u32) -> Option<u32>,
    {
        let mut previous = self.load(fetch_order);
        loop {
            let Some(next) = f(previous) else {
                return Err(previous);
            };

            match self.compare_exchange_weak(previous, next, set_order, fetch_order) {
                Ok(actual) => return Ok(actual),
                Err(actual) => previous = actual,
            }
        }
    }

    #[inline(always)]
    pub fn into_inner(self) -> u32 {
        self.value.into_inner()
    }
}

impl Default for AtomicU32 {
    #[inline(always)]
    fn default() -> Self {
        Self::new(0)
    }
}

pub struct AtomicBool {
    inner: AtomicU32,
}

impl AtomicBool {
    #[inline(always)]
    pub const fn new(value: bool) -> Self {
        Self {
            inner: AtomicU32::new(value as u32),
        }
    }

    #[inline(always)]
    pub fn load(&self, ordering: Ordering) -> bool {
        self.inner.load(ordering) != 0
    }

    #[inline(always)]
    pub fn store(&self, value: bool, ordering: Ordering) {
        self.inner.store(value as u32, ordering);
    }

    #[inline(always)]
    pub fn swap(&self, value: bool, ordering: Ordering) -> bool {
        self.inner.swap(value as u32, ordering) != 0
    }

    #[inline(always)]
    pub fn compare_exchange(
        &self,
        current: bool,
        new: bool,
        success: Ordering,
        failure: Ordering,
    ) -> Result<bool, bool> {
        self.inner
            .compare_exchange(current as u32, new as u32, success, failure)
            .map(|value| value != 0)
            .map_err(|value| value != 0)
    }

    #[inline(always)]
    pub fn compare_exchange_weak(
        &self,
        current: bool,
        new: bool,
        success: Ordering,
        failure: Ordering,
    ) -> Result<bool, bool> {
        self.inner
            .compare_exchange_weak(current as u32, new as u32, success, failure)
            .map(|value| value != 0)
            .map_err(|value| value != 0)
    }

    #[inline(always)]
    pub fn fetch_and(&self, value: bool, ordering: Ordering) -> bool {
        self.inner.fetch_and(value as u32, ordering) != 0
    }

    #[inline(always)]
    pub fn fetch_or(&self, value: bool, ordering: Ordering) -> bool {
        self.inner.fetch_or(value as u32, ordering) != 0
    }

    #[inline(always)]
    pub fn fetch_xor(&self, value: bool, ordering: Ordering) -> bool {
        self.inner.fetch_xor(value as u32, ordering) != 0
    }

    #[inline(always)]
    pub fn fetch_not(&self, ordering: Ordering) -> bool {
        self.inner.fetch_xor(1, ordering) != 0
    }

    #[inline(always)]
    pub fn into_inner(self) -> bool {
        self.inner.into_inner() != 0
    }
}

impl Default for AtomicBool {
    #[inline(always)]
    fn default() -> Self {
        Self::new(false)
    }
}

pub struct AtomicI32 {
    inner: AtomicU32,
}

impl AtomicI32 {
    #[inline(always)]
    pub const fn new(value: i32) -> Self {
        Self {
            inner: AtomicU32::new(value as u32),
        }
    }

    #[inline(always)]
    pub fn load(&self, ordering: Ordering) -> i32 {
        self.inner.load(ordering) as i32
    }

    #[inline(always)]
    pub fn store(&self, value: i32, ordering: Ordering) {
        self.inner.store(value as u32, ordering);
    }

    #[inline(always)]
    pub fn swap(&self, value: i32, ordering: Ordering) -> i32 {
        self.inner.swap(value as u32, ordering) as i32
    }

    #[inline(always)]
    pub fn compare_exchange(
        &self,
        current: i32,
        new: i32,
        success: Ordering,
        failure: Ordering,
    ) -> Result<i32, i32> {
        self.inner
            .compare_exchange(current as u32, new as u32, success, failure)
            .map(|value| value as i32)
            .map_err(|value| value as i32)
    }

    #[inline(always)]
    pub fn compare_exchange_weak(
        &self,
        current: i32,
        new: i32,
        success: Ordering,
        failure: Ordering,
    ) -> Result<i32, i32> {
        self.inner
            .compare_exchange_weak(current as u32, new as u32, success, failure)
            .map(|value| value as i32)
            .map_err(|value| value as i32)
    }

    #[inline(always)]
    pub fn fetch_add(&self, value: i32, ordering: Ordering) -> i32 {
        self.inner.fetch_add(value as u32, ordering) as i32
    }

    #[inline(always)]
    pub fn fetch_sub(&self, value: i32, ordering: Ordering) -> i32 {
        self.inner.fetch_sub(value as u32, ordering) as i32
    }

    #[inline(always)]
    pub fn fetch_and(&self, value: i32, ordering: Ordering) -> i32 {
        self.inner.fetch_and(value as u32, ordering) as i32
    }

    #[inline(always)]
    pub fn fetch_or(&self, value: i32, ordering: Ordering) -> i32 {
        self.inner.fetch_or(value as u32, ordering) as i32
    }

    #[inline(always)]
    pub fn fetch_xor(&self, value: i32, ordering: Ordering) -> i32 {
        self.inner.fetch_xor(value as u32, ordering) as i32
    }

    pub fn fetch_update<F>(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        mut f: F,
    ) -> Result<i32, i32>
    where
        F: FnMut(i32) -> Option<i32>,
    {
        self.inner
            .fetch_update(set_order, fetch_order, |value| {
                f(value as i32).map(|next| next as u32)
            })
            .map(|value| value as i32)
            .map_err(|value| value as i32)
    }

    #[inline(always)]
    pub fn into_inner(self) -> i32 {
        self.inner.into_inner() as i32
    }
}

impl Default for AtomicI32 {
    #[inline(always)]
    fn default() -> Self {
        Self::new(0)
    }
}

pub struct AtomicUsize {
    inner: AtomicU32,
}

impl AtomicUsize {
    #[inline(always)]
    pub const fn new(value: usize) -> Self {
        Self {
            inner: AtomicU32::new(value as u32),
        }
    }

    #[inline(always)]
    pub fn load(&self, ordering: Ordering) -> usize {
        self.inner.load(ordering) as usize
    }

    #[inline(always)]
    pub fn store(&self, value: usize, ordering: Ordering) {
        self.inner.store(value as u32, ordering);
    }

    #[inline(always)]
    pub fn swap(&self, value: usize, ordering: Ordering) -> usize {
        self.inner.swap(value as u32, ordering) as usize
    }

    #[inline(always)]
    pub fn compare_exchange(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.inner
            .compare_exchange(current as u32, new as u32, success, failure)
            .map(|value| value as usize)
            .map_err(|value| value as usize)
    }

    #[inline(always)]
    pub fn compare_exchange_weak(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.inner
            .compare_exchange_weak(current as u32, new as u32, success, failure)
            .map(|value| value as usize)
            .map_err(|value| value as usize)
    }

    #[inline(always)]
    pub fn fetch_add(&self, value: usize, ordering: Ordering) -> usize {
        self.inner.fetch_add(value as u32, ordering) as usize
    }

    #[inline(always)]
    pub fn fetch_sub(&self, value: usize, ordering: Ordering) -> usize {
        self.inner.fetch_sub(value as u32, ordering) as usize
    }

    #[inline(always)]
    pub fn fetch_and(&self, value: usize, ordering: Ordering) -> usize {
        self.inner.fetch_and(value as u32, ordering) as usize
    }

    #[inline(always)]
    pub fn fetch_or(&self, value: usize, ordering: Ordering) -> usize {
        self.inner.fetch_or(value as u32, ordering) as usize
    }

    #[inline(always)]
    pub fn fetch_xor(&self, value: usize, ordering: Ordering) -> usize {
        self.inner.fetch_xor(value as u32, ordering) as usize
    }

    pub fn fetch_update<F>(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        mut f: F,
    ) -> Result<usize, usize>
    where
        F: FnMut(usize) -> Option<usize>,
    {
        self.inner
            .fetch_update(set_order, fetch_order, |value| {
                f(value as usize).map(|next| next as u32)
            })
            .map(|value| value as usize)
            .map_err(|value| value as usize)
    }

    #[inline(always)]
    pub fn into_inner(self) -> usize {
        self.inner.into_inner() as usize
    }
}

impl Default for AtomicUsize {
    #[inline(always)]
    fn default() -> Self {
        Self::new(0)
    }
}

struct ArcInner<T> {
    strong: AtomicU32,
    data: T,
}

pub struct Arc<T> {
    inner: NonNull<ArcInner<T>>,
}

impl<T> Arc<T> {
    pub fn new(val: T) -> Self {
        let boxed = alloc::boxed::Box::new(ArcInner {
            strong: AtomicU32::new(1),
            data: val,
        });

        let raw = alloc::boxed::Box::into_raw(boxed);

        Self {
            inner: unsafe { NonNull::new_unchecked(raw) },
        }
    }

    #[inline(always)]
    fn inner_ref(&self) -> &ArcInner<T> {
        unsafe { self.inner.as_ref() }
    }

    #[inline(always)]
    pub fn strong_count(this: &Self) -> u32 {
        this.inner_ref().strong.load(Ordering::Acquire)
    }
}

impl<T> Clone for Arc<T> {
    fn clone(&self) -> Self {
        let previous = self.inner_ref().strong.fetch_add(1, Ordering::Relaxed);
        debug_assert!(previous > 0, "cloning an Arc with zero strong count");
        Self { inner: self.inner }
    }
}

impl<T> Deref for Arc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner_ref().data
    }
}

impl<T> Arc<T>
where
    T: Sized,
{
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        if Self::strong_count(this) == 1 {
            unsafe {
                let inner = this.inner.as_mut();
                Some(&mut inner.data)
            }
        } else {
            None
        }
    }
}

impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        let previous = self.inner_ref().strong.fetch_sub(1, Ordering::Release);
        debug_assert!(previous > 0, "dropping an Arc with zero strong count");

        if previous == 1 {
            fence_after(Ordering::Acquire);
            unsafe {
                drop(alloc::boxed::Box::from_raw(self.inner.as_ptr()));
            }
        }
    }
}

unsafe impl<T: Send + Sync> Send for Arc<T> {}
unsafe impl<T: Send + Sync> Sync for Arc<T> {}
