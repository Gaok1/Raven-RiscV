//! Realistic filesystem simulation for guest syscalls (`openat`, `read`,
//! `write`, `close`, `lseek`, `fstat`, `unlinkat`, `mkdirat`, ...).
//!
//! The guest's `/` maps onto a real host directory — the "fs root",
//! defaulting to the process's current working directory — like a chroot
//! jail. Every resolved path is checked to stay inside that root; `..` can
//! never walk above it, so a guest program can read, write, create and
//! delete real files, but only within the sandbox it was given.

use std::collections::HashMap;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

/// Linux `open(2)` flag bits we care about (octal, matching the generic ABI).
pub const O_WRONLY: u32 = 0o1;
pub const O_RDWR: u32 = 0o2;
pub const O_CREAT: u32 = 0o100;
pub const O_EXCL: u32 = 0o200;
pub const O_TRUNC: u32 = 0o1000;
pub const O_APPEND: u32 = 0o2000;
pub const O_DIRECTORY: u32 = 0o200000;

/// First fd handed out by [`FileSim::open`]; 0/1/2 stay reserved for the
/// console (stdin/stdout/stderr), which `FileSim` never touches.
const FIRST_FD: i32 = 3;

pub enum FsError {
    NotFound,
    PermissionDenied,
    IsADirectory,
    AlreadyExists,
    InvalidFd,
    Io,
}

impl From<std::io::Error> for FsError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind::*;
        match e.kind() {
            NotFound => FsError::NotFound,
            PermissionDenied => FsError::PermissionDenied,
            AlreadyExists => FsError::AlreadyExists,
            _ if e.raw_os_error() == Some(21) => FsError::IsADirectory, // EISDIR
            _ => FsError::Io,
        }
    }
}

pub struct FileStat {
    pub is_dir: bool,
    pub size: u64,
}

pub struct FileSim {
    root: PathBuf,
    files: HashMap<i32, File>,
    /// Directory fds opened with `O_DIRECTORY`: tracked for `close`/`fstat`
    /// but hold no read/write handle (matches real `openat` semantics —
    /// reading directory contents needs `getdents64`, which RAVEN doesn't
    /// model; listing isn't needed for the common "does this path exist,
    /// can I stat it" use case).
    dirs: HashMap<i32, PathBuf>,
    next_fd: i32,
}

impl Default for FileSim {
    fn default() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            root,
            files: HashMap::new(),
            dirs: HashMap::new(),
            next_fd: FIRST_FD,
        }
    }
}

impl FileSim {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn set_root(&mut self, root: PathBuf) {
        self.root = root;
    }

    /// Resolves a guest path (absolute-from-guest-root or cwd-relative — the
    /// guest's cwd is always the fs root) to a real host path. Rejects any
    /// path that would walk above the root via `..`.
    fn resolve(&self, guest_path: &str) -> Result<PathBuf, FsError> {
        let mut real = self.root.clone();
        for comp in Path::new(guest_path).components() {
            match comp {
                Component::Normal(part) => real.push(part),
                Component::CurDir => {}
                Component::ParentDir => {
                    if real == self.root {
                        return Err(FsError::PermissionDenied);
                    }
                    real.pop();
                }
                Component::RootDir | Component::Prefix(_) => {}
            }
        }
        Ok(real)
    }

    pub fn open(&mut self, guest_path: &str, flags: u32) -> Result<i32, FsError> {
        let path = self.resolve(guest_path)?;

        if flags & O_DIRECTORY != 0 {
            if !path.is_dir() {
                return Err(FsError::NotFound);
            }
            let fd = self.next_fd;
            self.next_fd += 1;
            self.dirs.insert(fd, path);
            return Ok(fd);
        }

        let writable = flags & (O_WRONLY | O_RDWR) != 0;
        let mut opts = OpenOptions::new();
        opts.read(!writable || flags & O_RDWR != 0);
        opts.write(writable);
        opts.append(flags & O_APPEND != 0);
        opts.truncate(writable && flags & O_TRUNC != 0);
        if flags & O_CREAT != 0 {
            if flags & O_EXCL != 0 {
                opts.create_new(true);
            } else {
                opts.create(true);
            }
        }

        let file = opts.open(&path)?;
        let fd = self.next_fd;
        self.next_fd += 1;
        self.files.insert(fd, file);
        Ok(fd)
    }

    /// Closes `fd` if it belongs to this filesystem. Returns `false` for an
    /// fd it doesn't know about (caller falls back to console/EBADF).
    pub fn close(&mut self, fd: i32) -> bool {
        self.files.remove(&fd).is_some() || self.dirs.remove(&fd).is_some()
    }

    pub fn has_fd(&self, fd: i32) -> bool {
        self.files.contains_key(&fd) || self.dirs.contains_key(&fd)
    }

    pub fn read(&mut self, fd: i32, buf: &mut [u8]) -> Result<usize, FsError> {
        let f = self.files.get_mut(&fd).ok_or(FsError::InvalidFd)?;
        Ok(f.read(buf)?)
    }

    pub fn write(&mut self, fd: i32, buf: &[u8]) -> Result<usize, FsError> {
        let f = self.files.get_mut(&fd).ok_or(FsError::InvalidFd)?;
        Ok(f.write(buf)?)
    }

    /// `whence`: 0 = SEEK_SET, 1 = SEEK_CUR, 2 = SEEK_END.
    pub fn seek(&mut self, fd: i32, offset: i64, whence: u32) -> Result<u64, FsError> {
        let f = self.files.get_mut(&fd).ok_or(FsError::InvalidFd)?;
        let pos = match whence {
            0 => SeekFrom::Start(offset.max(0) as u64),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => return Err(FsError::InvalidFd),
        };
        Ok(f.seek(pos)?)
    }

    pub fn stat_fd(&self, fd: i32) -> Result<FileStat, FsError> {
        if let Some(f) = self.files.get(&fd) {
            let meta = f.metadata()?;
            return Ok(to_file_stat(&meta));
        }
        if let Some(dir) = self.dirs.get(&fd) {
            let meta = std::fs::metadata(dir)?;
            return Ok(to_file_stat(&meta));
        }
        Err(FsError::InvalidFd)
    }

    pub fn exists(&self, guest_path: &str) -> bool {
        self.resolve(guest_path)
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    pub fn unlink(&self, guest_path: &str) -> Result<(), FsError> {
        let path = self.resolve(guest_path)?;
        if path.is_dir() {
            return Err(FsError::IsADirectory);
        }
        Ok(std::fs::remove_file(path)?)
    }

    pub fn mkdir(&self, guest_path: &str) -> Result<(), FsError> {
        let path = self.resolve(guest_path)?;
        if path.exists() {
            return Err(FsError::AlreadyExists);
        }
        Ok(std::fs::create_dir(path)?)
    }
}

fn to_file_stat(meta: &Metadata) -> FileStat {
    FileStat {
        is_dir: meta.is_dir(),
        size: meta.len(),
    }
}
