//! Run a recipe command so it can only write inside its staging folder.
//!
//! - Windows: the process gets a low-integrity copy of our token, and the staging
//!   folder is labelled low integrity. Windows' mandatory integrity policy (no
//!   write-up) then blocks writes everywhere else the user can normally write.
//! - Linux: a Landlock ruleset (kernel 5.13+) allows writes only beneath the staging
//!   folder and `/dev`. On older kernels the command still runs, with a warning in
//!   the log.
//!
//! Reads are not restricted: commands read their input wherever it is.

use anyhow::Result;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Output {
    /// Exit code; `None` when killed (cancelled).
    pub code: Option<i32>,
    pub stderr: String,
}

/// Keep at most this much stderr (the tail is what matters).
const STDERR_CAP: usize = 64 * 1024;

fn push_capped(buf: &mut Vec<u8>, chunk: &[u8]) {
    buf.extend_from_slice(chunk);
    if buf.len() > STDERR_CAP {
        let cut = buf.len() - STDERR_CAP;
        buf.drain(..cut);
    }
}

fn read_capped(mut r: impl std::io::Read) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    while let Ok(n) = r.read(&mut chunk) {
        if n == 0 {
            break;
        }
        push_capped(&mut buf, &chunk[..n]);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

pub fn run(argv: &[String], stage: &Path, cancel: &Arc<AtomicBool>) -> Result<Output> {
    imp::run(argv, stage, cancel)
}

pub fn confined() -> bool {
    #[cfg(windows)]
    return true;
    #[cfg(target_os = "linux")]
    return imp::landlock_supported();
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use landlock::{
        Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, ABI,
    };
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    pub fn run(argv: &[String], stage: &Path, cancel: &Arc<AtomicBool>) -> Result<Output> {
        let abi = ABI::V3;
        let ruleset = Ruleset::default()
            .handle_access(AccessFs::from_write(abi))?
            .create()?
            .add_rule(PathBeneath::new(
                PathFd::new(stage)?,
                AccessFs::from_all(abi),
            ))?
            .add_rule(PathBeneath::new(
                PathFd::new("/dev")?,
                AccessFs::from_write(abi),
            ))?;
        let mut ruleset = Some(ruleset);
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .current_dir(stage)
            .env("TMPDIR", stage)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        // SAFETY: restrict_self only makes the landlock/prctl syscalls on an fd that
        // was opened before fork.
        unsafe {
            cmd.pre_exec(move || {
                if let Some(r) = ruleset.take() {
                    // On kernels without Landlock this succeeds without enforcing;
                    // the parent logs that below.
                    r.restrict_self().map_err(std::io::Error::other)?;
                }
                Ok(())
            });
        }
        if !landlock_supported() {
            log::warn!("Landlock is unavailable on this kernel; recipe writes aren't confined");
        }
        let mut child = cmd.spawn()?;
        let err = child.stderr.take().unwrap();
        let reader = std::thread::spawn(move || read_capped(err));
        let code = loop {
            if let Some(st) = child.try_wait()? {
                break st.code();
            }
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        Ok(Output {
            code,
            stderr: reader.join().unwrap_or_default(),
        })
    }

    pub fn landlock_supported() -> bool {
        // landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION) returns the ABI version.
        let v = unsafe { libc_syscall_landlock_version() };
        v >= 1
    }

    unsafe fn libc_syscall_landlock_version() -> i64 {
        extern "C" {
            fn syscall(num: i64, ...) -> i64;
        }
        const SYS_LANDLOCK_CREATE_RULESET: i64 = 444;
        const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
        syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<u8>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use anyhow::{bail, Context};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows::core::{BOOL, PCWSTR, PWSTR};
    use windows::Win32::Foundation::*;
    use windows::Win32::Security::Authorization::*;
    use windows::Win32::Security::*;
    use windows::Win32::Storage::FileSystem::*;
    use windows::Win32::System::Pipes::CreatePipe;
    use windows::Win32::System::Threading::*;

    fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain(Some(0)).collect()
    }

    /// Quote one argument for CreateProcess (the MSVCRT rules).
    fn quote(arg: &str, out: &mut String) {
        if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
            out.push_str(arg);
            return;
        }
        out.push('"');
        let mut backslashes = 0;
        for c in arg.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                    out.push('"');
                    backslashes = 0;
                }
                c => {
                    out.extend(std::iter::repeat_n('\\', backslashes));
                    out.push(c);
                    backslashes = 0;
                }
            }
        }
        out.extend(std::iter::repeat_n('\\', backslashes * 2));
        out.push('"');
    }

    /// The CreateProcess command line. `cmd /c` reads the rest of the line itself
    /// (not with the C runtime rules), so what follows `/c` is passed as written.
    fn command_line(argv: &[String]) -> String {
        let is_cmd = std::path::Path::new(&argv[0])
            .file_stem()
            .is_some_and(|s| s.eq_ignore_ascii_case("cmd"));
        let mut line = String::new();
        let mut raw = false;
        for (i, a) in argv.iter().enumerate() {
            if i > 0 {
                line.push(' ');
            }
            if raw {
                line.push_str(a);
            } else {
                quote(a, &mut line);
            }
            raw |=
                is_cmd && i > 0 && (a.eq_ignore_ascii_case("/c") || a.eq_ignore_ascii_case("/k"));
        }
        line
    }

    /// Owned HANDLE closed on drop.
    struct H(HANDLE);
    impl Drop for H {
        fn drop(&mut self) {
            if !self.0.is_invalid() && !self.0 .0.is_null() {
                unsafe {
                    let _ = CloseHandle(self.0);
                }
            }
        }
    }

    /// Give `dir` (and what's created in it) a low mandatory integrity label.
    fn label_low(dir: &Path) -> Result<()> {
        unsafe {
            let mut sd = PSECURITY_DESCRIPTOR::default();
            let sddl = wide("S:(ML;OICI;NW;;;LW)");
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
            .context("building the low-integrity label")?;
            let mut present = BOOL(0);
            let mut defaulted = BOOL(0);
            let mut sacl: *mut ACL = std::ptr::null_mut();
            let r = GetSecurityDescriptorSacl(sd, &mut present, &mut sacl, &mut defaulted);
            let path = wide(dir);
            let e = if r.is_ok() {
                SetNamedSecurityInfoW(
                    PCWSTR(path.as_ptr()),
                    SE_FILE_OBJECT,
                    LABEL_SECURITY_INFORMATION,
                    None,
                    None,
                    None,
                    Some(sacl),
                )
            } else {
                WIN32_ERROR(1)
            };
            let _ = LocalFree(Some(HLOCAL(sd.0)));
            r.context("reading the label")?;
            e.ok().context("labelling the staging folder")?;
        }
        Ok(())
    }

    /// A primary token like ours, at low integrity.
    fn low_token() -> Result<H> {
        unsafe {
            let mut mine = HANDLE::default();
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY | TOKEN_ADJUST_DEFAULT,
                &mut mine,
            )?;
            let mine = H(mine);
            let mut dup = HANDLE::default();
            DuplicateTokenEx(
                mine.0,
                TOKEN_DUPLICATE
                    | TOKEN_QUERY
                    | TOKEN_ASSIGN_PRIMARY
                    | TOKEN_ADJUST_DEFAULT
                    | TOKEN_ADJUST_SESSIONID,
                None,
                SecurityImpersonation,
                TokenPrimary,
                &mut dup,
            )?;
            let dup = H(dup);
            let mut sid = PSID::default();
            let low = wide("S-1-16-4096");
            ConvertStringSidToSidW(PCWSTR(low.as_ptr()), &mut sid)?;
            let label = TOKEN_MANDATORY_LABEL {
                Label: SID_AND_ATTRIBUTES {
                    Sid: sid,
                    Attributes: 0x20, // SE_GROUP_INTEGRITY
                },
            };
            let r = SetTokenInformation(
                dup.0,
                TokenIntegrityLevel,
                &label as *const _ as *const _,
                (std::mem::size_of::<TOKEN_MANDATORY_LABEL>() + GetLengthSid(sid) as usize) as u32,
            );
            let _ = LocalFree(Some(HLOCAL(sid.0)));
            r?;
            Ok(dup)
        }
    }

    /// Environment block with TEMP/TMP pointing at the staging folder (low processes
    /// can't write to the normal temp folder).
    fn environment(stage: &Path) -> Vec<u16> {
        let mut vars: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| !k.eq_ignore_ascii_case("TEMP") && !k.eq_ignore_ascii_case("TMP"))
            .collect();
        let s = stage.display().to_string();
        vars.push(("TEMP".into(), s.clone()));
        vars.push(("TMP".into(), s));
        vars.sort_by_key(|(k, _)| k.to_uppercase());
        let mut block = Vec::new();
        for (k, v) in vars {
            block.extend(format!("{k}={v}").encode_utf16());
            block.push(0);
        }
        block.push(0);
        block
    }

    pub fn run(argv: &[String], stage: &Path, cancel: &Arc<AtomicBool>) -> Result<Output> {
        label_low(stage)?;
        let token = low_token().context("creating a low-integrity token")?;
        let cmdline = command_line(argv);
        let mut cmdline: Vec<u16> = cmdline.encode_utf16().chain(Some(0)).collect();
        let env = environment(stage);
        let cwd = wide(stage);
        unsafe {
            let inherit = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: std::ptr::null_mut(),
                bInheritHandle: TRUE,
            };
            let (mut rd, mut wr) = (HANDLE::default(), HANDLE::default());
            CreatePipe(&mut rd, &mut wr, Some(&inherit), 0)?;
            let (rd, wr) = (H(rd), H(wr));
            SetHandleInformation(rd.0, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0))?;
            let nul_name = wide("NUL");
            let nul = H(CreateFileW(
                PCWSTR(nul_name.as_ptr()),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                Some(&inherit),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )?);
            let si = STARTUPINFOW {
                cb: std::mem::size_of::<STARTUPINFOW>() as u32,
                dwFlags: STARTF_USESTDHANDLES,
                hStdInput: nul.0,
                hStdOutput: nul.0,
                hStdError: wr.0,
                ..Default::default()
            };
            let mut pi = PROCESS_INFORMATION::default();
            CreateProcessAsUserW(
                Some(token.0),
                PCWSTR::null(),
                Some(PWSTR(cmdline.as_mut_ptr())),
                None,
                None,
                true,
                CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
                Some(env.as_ptr() as *const _),
                PCWSTR(cwd.as_ptr()),
                &si,
                &mut pi,
            )
            .with_context(|| format!("`{}` (is it installed and on PATH?)", argv[0]))?;
            let process = H(pi.hProcess);
            drop(H(pi.hThread));
            // Our copy of the write end must close so the reader sees EOF.
            drop(wr);
            drop(nul);
            let file = std::fs::File::from_raw_handle(rd.0 .0 as _);
            std::mem::forget(rd);
            let reader = std::thread::spawn(move || read_capped(file));
            let code = loop {
                if WaitForSingleObject(process.0, 50) == WAIT_OBJECT_0 {
                    let mut code = 0u32;
                    GetExitCodeProcess(process.0, &mut code)?;
                    break Some(code as i32);
                }
                if cancel.load(Ordering::Relaxed) {
                    let _ = TerminateProcess(process.0, 1);
                    let _ = WaitForSingleObject(process.0, 5000);
                    break None;
                }
            };
            let stderr = reader.join().unwrap_or_default();
            if code.is_none() && !cancel.load(Ordering::Relaxed) {
                bail!("process lost");
            }
            Ok(Output { code, stderr })
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn quoting() {
            let mut s = String::new();
            for a in [
                "plain",
                "with space",
                r#"say "hi""#,
                r"C:\dir\",
                r"C:\my dir\",
                "",
            ] {
                super::quote(a, &mut s);
                s.push(' ');
            }
            assert_eq!(
                s,
                r#"plain "with space" "say \"hi\"" C:\dir\ "C:\my dir\\" "" "#
            );
            let argv: Vec<String> = ["cmd", "/c", r#"copy "a b" c"#].map(String::from).into();
            assert_eq!(super::command_line(&argv), r#"cmd /c copy "a b" c"#);
        }
    }
}
