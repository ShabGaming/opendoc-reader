//! Child process containment (M1-9): children must not outlive the app.
//!
//! Windows: each child gets its own Job Object with `KILL_ON_JOB_CLOSE`, so the whole process
//! tree dies when the handle closes, which also happens when the app is killed. (A virtualenv's
//! `python.exe` on Windows is a launcher that starts the real interpreter as its child; the job
//! covers both.) Unix: each child leads its own process group, and the group is killed.

use std::io;

use tokio::process::{Child, Command};

pub struct Contained {
    pub child: Child,
    #[cfg(windows)]
    job: Option<win::Job>,
}

impl Contained {
    /// Starts the command inside its own containment. The command's stdio setup is the
    /// caller's business.
    pub fn spawn(mut command: Command) -> io::Result<Self> {
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: no console window flashes up for a console program.
            command.creation_flags(0x0800_0000);
        }
        #[cfg(unix)]
        {
            command.process_group(0);
        }
        command.kill_on_drop(true);
        let child = command.spawn()?;
        #[cfg(windows)]
        {
            let job = win::Job::new().filter(|job| job.assign(&child));
            if job.is_none() {
                tracing::warn!("could not put a child process into a job object");
            }
            Ok(Self { child, job })
        }
        #[cfg(not(windows))]
        Ok(Self { child })
    }

    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Kills the child and everything it started, and reaps it.
    pub async fn kill(&mut self) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
        }
        #[cfg(unix)]
        if let Some(pid) = self.child.id() {
            // SAFETY: killpg on the group this child leads.
            unsafe {
                libc::killpg(pid as libc::pid_t, libc::SIGKILL);
            }
        }
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

#[cfg(unix)]
impl Drop for Contained {
    fn drop(&mut self) {
        if let Some(pid) = self.child.id() {
            // SAFETY: killpg on the group this child leads.
            unsafe {
                libc::killpg(pid as libc::pid_t, libc::SIGKILL);
            }
        }
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    use tokio::process::Child;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct Job(HANDLE);

    // SAFETY: a job handle is just a kernel object reference; any thread may use or close it.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        pub fn new() -> Option<Self> {
            // SAFETY: plain Win32 calls with valid arguments; the handle is closed in Drop.
            unsafe {
                let handle = CreateJobObjectW(None, PCWSTR::null()).ok()?;
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if set.is_err() {
                    let _ = CloseHandle(handle);
                    return None;
                }
                Some(Job(handle))
            }
        }

        pub fn assign(&self, child: &Child) -> bool {
            let Some(process) = child.raw_handle() else {
                return false;
            };
            // SAFETY: `process` is the live handle of a child we own.
            unsafe { AssignProcessToJobObject(self.0, HANDLE(process)).is_ok() }
        }

        pub fn terminate(&self) {
            // SAFETY: the job handle is valid until Drop.
            unsafe {
                let _ = TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: closing the handle we created; KILL_ON_JOB_CLOSE ends the processes in it.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    fn sleeper() -> Command {
        let mut command = if cfg!(windows) {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 60"]);
            c
        } else {
            let mut c = Command::new("sleep");
            c.arg("60");
            c
        };
        command.stdin(Stdio::null()).stdout(Stdio::null());
        command
    }

    #[tokio::test]
    async fn kill_ends_the_child_and_reaps_it() {
        let mut contained = Contained::spawn(sleeper()).unwrap();
        assert!(contained.id().is_some());
        contained.kill().await;
        assert!(contained.child.try_wait().unwrap().is_some());
    }

    #[tokio::test]
    async fn dropping_the_handle_kills_the_child() {
        let contained = Contained::spawn(sleeper()).unwrap();
        let pid = contained.id().unwrap();
        drop(contained);
        // The process may take a moment to disappear.
        let mut gone = false;
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let system = sysinfo::System::new_all();
            // A killed child that nobody has reaped yet shows up as a zombie on Unix.
            let alive = system
                .process(sysinfo::Pid::from_u32(pid))
                .is_some_and(|p| p.status() != sysinfo::ProcessStatus::Zombie);
            if !alive {
                gone = true;
                break;
            }
        }
        assert!(gone, "the child process is still running after the drop");
    }
}
