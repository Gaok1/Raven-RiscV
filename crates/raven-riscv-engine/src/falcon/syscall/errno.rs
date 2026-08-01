//! Linux errno values, as returned in `a0` (`-errno`, represented as `u32`).

pub const LINUX_ENOENT: u32 = (-2i32) as u32;
pub const LINUX_EIO: u32 = (-5i32) as u32;
pub const LINUX_EBADF: u32 = (-9i32) as u32;
pub const LINUX_ENOMEM: u32 = (-12i32) as u32;
pub const LINUX_EFAULT: u32 = (-14i32) as u32;
pub const LINUX_EINVAL: u32 = (-22i32) as u32;
pub const LINUX_ENOTTY: u32 = (-25i32) as u32;
pub const LINUX_ESPIPE: u32 = (-29i32) as u32;
pub const LINUX_ENOSYS: u32 = (-38i32) as u32;
