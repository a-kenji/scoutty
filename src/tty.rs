use std::io;
use std::os::unix::io::{AsFd, AsRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicI32, AtomicPtr, Ordering};
use std::time::Instant;

use nix::libc;
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
use nix::sys::termios::{self, SetArg, Termios};
use nix::unistd;

static ORIGINAL_TERMIOS: AtomicPtr<Termios> = AtomicPtr::new(std::ptr::null_mut());
static TTY_FD: AtomicI32 = AtomicI32::new(-1);

pub struct Tty {
    fd: OwnedFd,
}

impl Tty {
    pub fn open() -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")?;
        Ok(Tty { fd: file.into() })
    }

    pub fn raw_mode(&self) -> io::Result<RawModeGuard> {
        let raw_fd = self.fd.as_raw_fd();
        let original = termios::tcgetattr(&self.fd).map_err(io::Error::other)?;

        // Store original termios for signal handler
        let boxed = Box::new(original.clone());
        let ptr = Box::into_raw(boxed);
        let old = ORIGINAL_TERMIOS.swap(ptr, Ordering::SeqCst);
        if !old.is_null() {
            // SAFETY: `old` was produced by `Box::into_raw` in a previous call to
            // `raw_mode()`, and the `swap` above ensures no other thread can access
            // this pointer. We are the sole owner, so reconstructing the Box is sound.
            unsafe {
                drop(Box::from_raw(old));
            }
        }
        TTY_FD.store(raw_fd, Ordering::SeqCst);

        install_signal_handlers();

        // Install panic hook to restore termios
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_termios_from_global();
            prev_hook(info);
        }));

        let mut raw = original.clone();
        termios::cfmakeraw(&mut raw);
        termios::tcsetattr(&self.fd, SetArg::TCSANOW, &raw).map_err(io::Error::other)?;

        Ok(RawModeGuard {
            fd: raw_fd,
            original,
        })
    }

    pub fn write_all(&self, buf: &[u8]) -> io::Result<()> {
        let mut written = 0;
        while written < buf.len() {
            match unistd::write(self.fd.as_fd(), &buf[written..]) {
                Ok(n) => written += n,
                Err(nix::errno::Errno::EINTR) => continue,
                Err(e) => return Err(io::Error::from(e)),
            }
        }
        Ok(())
    }

    pub fn poll_read(&self, buf: &mut [u8], deadline: Instant) -> io::Result<usize> {
        let now = Instant::now();
        if now >= deadline {
            return Ok(0);
        }
        let remaining = deadline - now;
        let timeout_ms = remaining.as_millis().min(i32::MAX as u128) as i32;

        let timeout_u16 = (timeout_ms as u32).min(u16::MAX as u32) as u16;
        let mut pollfd = [PollFd::new(self.fd.as_fd(), PollFlags::POLLIN)];
        match poll(&mut pollfd, PollTimeout::from(timeout_u16)) {
            Ok(0) => Ok(0),
            Ok(_) => match unistd::read(self.fd.as_fd(), buf) {
                Ok(n) => Ok(n),
                Err(nix::errno::Errno::EINTR) => Ok(0),
                Err(e) => Err(io::Error::from(e)),
            },
            Err(nix::errno::Errno::EINTR) => Ok(0),
            Err(e) => Err(io::Error::from(e)),
        }
    }
}

pub struct RawModeGuard {
    fd: RawFd,
    original: Termios,
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // SAFETY: `self.fd` is a valid file descriptor obtained from `OwnedFd::as_raw_fd()`
        // during `raw_mode()`. The `OwnedFd` (and thus the underlying fd) is still alive
        // because `RawModeGuard` is dropped before `Tty` (it borrows the fd, not owns it).
        // The `BorrowedFd` does not outlive this statement.
        let _ = termios::tcsetattr(
            unsafe { std::os::unix::io::BorrowedFd::borrow_raw(self.fd) },
            SetArg::TCSANOW,
            &self.original,
        );
        let ptr = ORIGINAL_TERMIOS.swap(std::ptr::null_mut(), Ordering::SeqCst);
        if !ptr.is_null() {
            // SAFETY: `ptr` was produced by `Box::into_raw` in `raw_mode()`, and the
            // atomic `swap` with null above ensures exclusive ownership. No signal
            // handler or other thread can access this pointer after the swap.
            unsafe {
                drop(Box::from_raw(ptr));
            }
        }
        TTY_FD.store(-1, Ordering::SeqCst);
    }
}

fn restore_termios_from_global() {
    let fd = TTY_FD.load(Ordering::SeqCst);
    let ptr = ORIGINAL_TERMIOS.load(Ordering::SeqCst);
    if fd >= 0 && !ptr.is_null() {
        // SAFETY: `ptr` was created via `Box::into_raw` in `raw_mode()` and remains
        // valid until `RawModeGuard::drop` swaps it to null. This function is called
        // from signal handlers and panic hooks *before* the guard is dropped, so the
        // pointer is still valid. We only read through it (shared reference), which
        // is safe even if called concurrently from a signal handler.
        let termios = unsafe { &*ptr };
        // SAFETY: `fd` was stored from a valid `OwnedFd::as_raw_fd()` in `raw_mode()`
        // and is only reset to -1 in `RawModeGuard::drop`. Since we checked `fd >= 0`,
        // the fd is still open. The `BorrowedFd` does not outlive this expression.
        let _ = nix::sys::termios::tcsetattr(
            unsafe { std::os::unix::io::BorrowedFd::borrow_raw(fd) },
            SetArg::TCSANOW,
            termios,
        );
    }
}

extern "C" fn signal_handler(sig: libc::c_int) {
    restore_termios_from_global();
    // SAFETY: These are async-signal-safe libc functions. We reset the signal
    // disposition to SIG_DFL and re-raise so the process terminates with the
    // correct signal status (allowing the parent to observe the right exit code).
    // `sig` is the signal number passed by the kernel, so it is always valid.
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

fn install_signal_handlers() {
    // SAFETY: `sigaction` requires an unsafe block because it modifies
    // process-wide signal handling state. Our `signal_handler` is
    // async-signal-safe (it only calls `restore_termios_from_global`,
    // `libc::signal`, and `libc::raise`). The `SigAction` is correctly
    // constructed with a valid handler function pointer.
    unsafe {
        let action = SigAction::new(
            SigHandler::Handler(signal_handler),
            SaFlags::empty(),
            SigSet::empty(),
        );
        let _ = sigaction(Signal::SIGINT, &action);
        let _ = sigaction(Signal::SIGTERM, &action);
    }
}
