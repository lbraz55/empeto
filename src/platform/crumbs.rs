//! Rastros de inicialização (diagnóstico): cada etapa grava uma linha em `files/startup.log`.
//! Falhas nativas (sinais) são gravadas em `files/crash.txt` antes de o processo terminar.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// Acrescenta uma linha ao rastro (com sincronização em disco: sobrevive a travamentos).
pub fn crumb(msg: &str) {
    log::info!("rastro: {msg}");
    let Some(dir) = DIR.get() else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("startup.log")) {
        let t = chrono::Local::now().format("%H:%M:%S%.3f");
        let _ = writeln!(f, "{t} [rust] {msg}");
        let _ = f.sync_all();
    }
}

#[cfg(target_os = "android")]
pub fn install_signal_handlers() {
    use std::ffi::CString;
    let Some(dir) = DIR.get() else { return };
    let path = CString::new(dir.join("crash.txt").to_string_lossy().as_bytes()).unwrap_or_default();
    // SAFETY: open/sigaction com argumentos válidos; o handler só usa funções async-signal-safe.
    unsafe {
        let fd = libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND, 0o600);
        if fd < 0 {
            return;
        }
        CRASH_FD = fd;
        for sig in [libc::SIGSEGV, libc::SIGABRT, libc::SIGBUS, libc::SIGILL, libc::SIGFPE] {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = handler as *const () as usize;
            sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(sig, &sa, &mut OLD[sig as usize]);
        }
    }
}

#[cfg(target_os = "android")]
static mut CRASH_FD: i32 = -1;
#[cfg(target_os = "android")]
static mut OLD: [libc::sigaction; 32] = unsafe { std::mem::zeroed() };

#[cfg(target_os = "android")]
extern "C" fn handler(sig: i32, _info: *mut libc::siginfo_t, _ctx: *mut libc::c_void) {
    let name: &[u8] = match sig {
        libc::SIGSEGV => b"SIGSEGV (acesso invalido de memoria)",
        libc::SIGABRT => b"SIGABRT (abort)",
        libc::SIGBUS => b"SIGBUS",
        libc::SIGILL => b"SIGILL",
        libc::SIGFPE => b"SIGFPE",
        _ => b"sinal",
    };
    // SAFETY: apenas write/sigaction/raise (async-signal-safe).
    unsafe {
        let fd = CRASH_FD;
        if fd >= 0 {
            let pre = b"falha nativa: ";
            libc::write(fd, pre.as_ptr() as *const _, pre.len());
            libc::write(fd, name.as_ptr() as *const _, name.len());
            libc::write(fd, b"\n".as_ptr() as *const _, 1);
            libc::fsync(fd);
        }
        // Devolve ao tratador anterior (debuggerd) e reenvia o sinal.
        libc::sigaction(sig, &OLD[sig as usize], std::ptr::null_mut());
        libc::raise(sig);
    }
}
