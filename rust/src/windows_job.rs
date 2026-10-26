//! Private Windows process ownership. The primary thread stays suspended until
//! assignment to a non-inheritable kill-on-close job succeeds. No PID-based
//! termination, breakaway flag, shell, or fallback to an uncontained process.
use std::{
    collections::BTreeMap,
    ffi::{c_void, OsStr, OsString},
    fs::File,
    io,
    mem::{size_of, size_of_val, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    process::{Command, ExitStatus},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{
        DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE as Handle, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{JobObjects::*, Threading::*},
};
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut out = value.encode_wide().collect::<Vec<_>>();
    if out.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NUL in procesargument",
        ));
    }
    out.push(0);
    Ok(out)
}
fn quote(value: &OsStr, out: &mut Vec<u16>) -> io::Result<()> {
    out.push(b'"' as u16);
    let mut slashes = 0;
    for ch in value.encode_wide() {
        if ch == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NUL in procesargument",
            ));
        }
        if ch == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        if ch == b'"' as u16 {
            out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2 + 1));
        } else {
            out.extend(std::iter::repeat_n(b'\\' as u16, slashes));
        }
        slashes = 0;
        out.push(ch);
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    out.push(b'"' as u16);
    Ok(())
}
fn owned(handle: Handle) -> io::Result<OwnedHandle> {
    if handle.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}
fn duplicate(handle: Handle) -> io::Result<OwnedHandle> {
    let mut target = null_mut();
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut target,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    owned(target)
}
struct Attributes {
    memory: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn new(handles: &mut [Handle]) -> io::Result<Self> {
        let mut bytes = 0;
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut value = Self {
            memory: vec![0; bytes.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        if unsafe { InitializeProcThreadAttributeList(value.ptr(), 1, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        value.initialized = true;
        if unsafe {
            UpdateProcThreadAttribute(
                value.ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_mut_ptr().cast(),
                size_of_val(handles),
                null_mut(),
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(value)
    }
    fn ptr(&mut self) -> *mut c_void {
        self.memory.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { DeleteProcThreadAttributeList(self.ptr()) }
        }
    }
}
pub(crate) struct Process {
    process: OwnedHandle,
    job: Option<OwnedHandle>,
    pid: u32,
}
impl Process {
    /// `command` supplies program/arguments/current directory/environment. This
    /// worker owns explicit log/NUL stdio; arbitrary Command stdio is not used.
    pub(crate) fn spawn(command: &Command, log: &File) -> io::Result<Self> {
        Self::spawn_assigned(command, log, |job, process| {
            if unsafe { AssignProcessToJobObject(job, process) } == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    }
    fn spawn_assigned(
        command: &Command,
        log: &File,
        assign: impl FnOnce(Handle, Handle) -> io::Result<()>,
    ) -> io::Result<Self> {
        let job = owned(unsafe { CreateJobObjectW(null(), null()) })?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let nul = File::open("NUL")?;
        let input = duplicate(nul.as_raw_handle())?;
        let output = duplicate(log.as_raw_handle())?;
        let mut handles = [input.as_raw_handle(), output.as_raw_handle()];
        let mut attrs = Attributes::new(&mut handles)?;
        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = input.as_raw_handle();
        startup.StartupInfo.hStdOutput = output.as_raw_handle();
        startup.StartupInfo.hStdError = output.as_raw_handle();
        startup.lpAttributeList = attrs.ptr();
        let executable = std::fs::canonicalize(command.get_program())?;
        let application = wide(executable.as_os_str())?;
        let mut line = Vec::new();
        quote(command.get_program(), &mut line)?;
        for arg in command.get_args() {
            line.push(b' ' as u16);
            quote(arg, &mut line)?;
        }
        line.push(0);
        let cwd = command
            .get_current_dir()
            .map(std::path::Path::to_path_buf)
            .unwrap_or(std::env::current_dir()?);
        let cwd = wide(cwd.as_os_str())?;
        let mut variables = std::env::vars_os()
            .map(|(k, v)| (k.to_string_lossy().to_uppercase(), v))
            .collect::<BTreeMap<String, OsString>>();
        for (k, v) in command.get_envs() {
            let key = k.to_string_lossy().to_uppercase();
            if let Some(v) = v {
                variables.insert(key, v.into());
            } else {
                variables.remove(&key);
            }
        }
        let mut env = Vec::new();
        for (k, v) in variables {
            env.extend(OsStr::new(&k).encode_wide());
            env.push(b'=' as u16);
            let value = wide(&v)?;
            env.extend(value);
        }
        env.push(0);
        if env.len() == 1 {
            env.push(0);
        }
        let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
        if unsafe {
            CreateProcessW(
                application.as_ptr(),
                line.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_NO_WINDOW
                    | EXTENDED_STARTUPINFO_PRESENT
                    | CREATE_UNICODE_ENVIRONMENT
                    | CREATE_SUSPENDED,
                env.as_ptr().cast(),
                cwd.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let value = Self {
            process: owned(info.hProcess)?,
            job: Some(job),
            pid: info.dwProcessId,
        };
        let thread = owned(info.hThread)?;
        assign(
            value.job.as_ref().unwrap().as_raw_handle(),
            value.process.as_raw_handle(),
        )
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Eigen procesboom kon niet worden afgeschermd (Windows-jobbeperking): {e}"),
            )
        })?;
        if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(value)
    }
    pub(crate) fn id(&self) -> u32 {
        self.pid
    }
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(ExitStatus::from_raw(code)))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), u32::MAX) } != 0 {
            return Err(io::Error::last_os_error());
        }
        self.try_wait()?
            .ok_or_else(|| io::Error::other("Processtatus ontbreekt"))
    }
    pub(crate) fn kill(&mut self) -> io::Result<()> {
        if let Some(job) = &self.job {
            if unsafe { TerminateJobObject(job.as_raw_handle(), 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            unsafe {
                TerminateJobObject(job.as_raw_handle(), 1);
                TerminateProcess(self.process.as_raw_handle(), 1);
                WaitForSingleObject(self.process.as_raw_handle(), 5000);
            }
            drop(job);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };
    const FIXTURE: &str = "vllm_runtime::windows_job::tests::native_tree_fixture";
    fn fixture(dir: &Path, role: &str) -> Command {
        let mut c = Command::new(std::env::current_exe().unwrap());
        c.args(["--exact", FIXTURE, "--nocapture"])
            .env("SLEUFBASE_JOB_FIXTURE_DIR", dir)
            .env("SLEUFBASE_JOB_FIXTURE_ROLE", role);
        c
    }
    // A native child of this test executable, never a shell/GPU/model runtime.
    #[test]
    fn native_tree_fixture() {
        let Some(dir) = std::env::var_os("SLEUFBASE_JOB_FIXTURE_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let role = std::env::var("SLEUFBASE_JOB_FIXTURE_ROLE").unwrap();
        if role == "exit7" {std::process::exit(7);}
        std::fs::write(
            dir.join(format!("{role}.pid")),
            std::process::id().to_string(),
        )
        .unwrap();
        let next = match role.as_str() {
            "parent" | "exiting" => Some("child"),
            "child" => Some("grandchild"),
            _ => None,
        };
        let _child = next.map(|next| {
            let mut c = fixture(&dir, next);
            c.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            c.spawn().unwrap()
        });
        if role == "exiting" {
            return;
        }
        std::thread::sleep(Duration::from_secs(30));
    }
    fn wait_pid(dir: &Path, role: &str) -> OwnedHandle {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(text) = std::fs::read_to_string(dir.join(format!("{role}.pid"))) {
                if let Ok(pid) = text.parse() {
                    let h = unsafe {
                        OpenProcess(
                            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                            0,
                            pid,
                        )
                    };
                    if !h.is_null() {
                        return owned(h).unwrap();
                    }
                }
            }
            assert!(Instant::now() < deadline, "fixture did not start: {role}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn drop_terminates_native_child_and_grandchild_even_after_root_exit() {
        for role in ["parent", "exiting"] {
            let dir = tempfile::tempdir().unwrap();
            let log = tempfile::tempfile().unwrap();
            let mut process = Process::spawn(&fixture(dir.path(), role), &log).unwrap();
            let child = wait_pid(dir.path(), "child");
            let grandchild = wait_pid(dir.path(), "grandchild");
            if role == "exiting" {
                assert!(process.wait().unwrap().success());
            }
            drop(process);
            for handle in [&child, &grandchild] {
                assert_eq!(
                    unsafe { WaitForSingleObject(handle.as_raw_handle(), 5000) },
                    WAIT_OBJECT_0,
                    "owned descendant survived job close"
                );
            }
        }
    }
    #[test]
    fn startup_cancellation_terminates_native_descendants() {
        use super::super::{Config, OwnedChild, Runtime};
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let log = tempfile::tempfile().unwrap();
        let cancel = AtomicBool::new(false);
        let runtime = Runtime::default();
        let config = Config::from_environment(dir.path().into(), BTreeMap::new());
        let mut calls = 0;
        let mut descendants = Vec::new();
        let result = runtime.ensure_with(
            &config,
            &cancel,
            || {
                calls += 1;
                if calls == 3 {
                    descendants.push(wait_pid(dir.path(), "child"));
                    descendants.push(wait_pid(dir.path(), "grandchild"));
                    cancel.store(true, Ordering::Release);
                }
                false
            },
            || {
                Ok(OwnedChild(
                    Process::spawn(&fixture(dir.path(), "parent"), &log).unwrap(),
                ))
            },
            Duration::ZERO,
        );
        assert!(result.is_err());
        assert!(runtime.child.lock().unwrap().is_none());
        assert_eq!(descendants.len(), 2);
        for h in descendants {
            assert_eq!(
                unsafe { WaitForSingleObject(h.as_raw_handle(), 5000) },
                WAIT_OBJECT_0
            );
        }
    }
    #[test]
    fn failed_assignment_never_runs_child_code() {
        let dir = tempfile::tempdir().unwrap();
        let log = tempfile::tempfile().unwrap();
        let mut handle = None;
        let result = Process::spawn_assigned(&fixture(dir.path(), "parent"), &log, |_, process| {
            handle = Some(duplicate(process).unwrap());
            Err(io::Error::other("injected incompatible outer job"))
        });
        assert!(result.is_err());
        assert!(!dir.path().join("parent.pid").exists());
        assert_eq!(
            unsafe { WaitForSingleObject(handle.unwrap().as_raw_handle(), 5000) },
            WAIT_OBJECT_0
        );
    }
    #[test]
    fn closing_owned_job_does_not_terminate_unowned_native_process() {
        let owned_dir = tempfile::tempdir().unwrap();
        let outside_dir = tempfile::tempdir().unwrap();
        let log = tempfile::tempfile().unwrap();
        let mut c = fixture(outside_dir.path(), "outside");
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        struct Control(std::process::Child);
        impl Drop for Control {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut outside = Control(c.spawn().unwrap());
        let _outside_handle = wait_pid(outside_dir.path(), "outside");
        let process = Process::spawn(&fixture(owned_dir.path(), "grandchild"), &log).unwrap();
        let handle = wait_pid(owned_dir.path(), "grandchild");
        drop(process);
        assert_eq!(
            unsafe { WaitForSingleObject(handle.as_raw_handle(), 5000) },
            WAIT_OBJECT_0
        );
        assert!(outside.0.try_wait().unwrap().is_none());
    }
}
