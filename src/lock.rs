use rustix::fs::{flock, FlockOperation};
use rustix::io::Errno;
use std::fs::{File, OpenOptions};
use std::os::fd::AsFd;
use std::path::PathBuf;

pub struct SingleInstanceLock {
    _file: File,
}

impl SingleInstanceLock {
    pub fn acquire() -> Option<Self> {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
            let uid = rustix::process::getuid().as_raw();
            format!("/run/user/{}", uid)
        });

        let lock_path = PathBuf::from(&runtime_dir).join("ssnip.lock");
        let lock_path = if lock_path.parent().is_some_and(|p| p.exists()) {
            lock_path
        } else {
            let uid = rustix::process::getuid().as_raw();
            PathBuf::from("/tmp").join(format!("ssnip_{}.lock", uid))
        };

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .ok()?;

        match flock(file.as_fd(), FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Some(Self { _file: file }),
            Err(Errno::WOULDBLOCK) => None,
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_instance_lock() {
        let lock1 = SingleInstanceLock::acquire();
        assert!(lock1.is_some(), "First lock should succeed");

        let lock2 = SingleInstanceLock::acquire();
        assert!(lock2.is_none(), "Second concurrent lock must fail");

        drop(lock1);

        let lock3 = SingleInstanceLock::acquire();
        assert!(lock3.is_some(), "Lock can be reacquired after release");
    }
}
