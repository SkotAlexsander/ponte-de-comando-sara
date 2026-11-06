//! Um Job Object do Windows para TODOS os processos filhos (sidecar, claude, testes de chave, login):
//! com KILL_ON_JOB_CLOSE, fechar a Ponte (até à força) encerra tudo que ela abriu.

use std::sync::OnceLock;

static JOB: OnceLock<usize> = OnceLock::new();

#[cfg(windows)]
fn criar() -> usize {
    use windows_sys::Win32::System::JobObjects::*;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return 0;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as *const core::ffi::c_void, std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32);
        job as usize
    }
}

/// Põe o processo no Job da Ponte. Devolve false se não deu (o chamador registra).
#[cfg(windows)]
pub fn amarrar(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};
    let job = *JOB.get_or_init(criar);
    if job == 0 {
        return false;
    }
    unsafe {
        let h = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = AssignProcessToJobObject(job as _, h) != 0;
        CloseHandle(h);
        ok
    }
}

#[cfg(not(windows))]
pub fn amarrar(_pid: u32) -> bool {
    false
}

/// O claude.exe instalado, sempre por caminho absoluto.
pub fn claude_exe() -> Option<std::path::PathBuf> {
    let p = std::path::PathBuf::from(std::env::var("USERPROFILE").ok()?).join(".local").join("bin").join("claude.exe");
    p.is_file().then_some(p)
}
