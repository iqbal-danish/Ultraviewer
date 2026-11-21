use crossbeam_channel::Receiver;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonRunStatus {
    Idle,
    Running {
        started_at: Instant,
        pid: u32,
    },
    Finished {
        exit_code: Option<i32>,
        duration: Duration,
        success: bool,
    },
    Terminated {
        duration: Duration,
    },
    FailedToStart(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputChunkKind {
    System,
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub struct OutputLine {
    pub kind: OutputChunkKind,
    pub text: String,
    pub timestamp: Instant,
}

pub enum OutputMessage {
    Line(OutputLine),
    TruncationReached {
        preview_limit: usize,
    },
    StdoutFinished {
        total_lines: usize,
        total_bytes: u64,
    },
    Finished {
        exit_code: Option<i32>,
        duration: Duration,
        success: bool,
    },
    Terminated {
        duration: Duration,
    },
    FailedToStart(String),
}

pub struct PythonEnvironment;

impl PythonEnvironment {
    /// Discovers the best Python interpreter available.
    /// Priority:
    /// 1. Custom path override if set and valid.
    /// 2. Workspace / parent directory virtual environments (`.venv`, `venv`, `env`).
    /// 3. System `PATH` (`python`, `python3`, `py`).
    pub fn find_interpreter(
        custom_path: Option<&str>,
        working_dir: Option<&Path>,
    ) -> Option<PathBuf> {
        if let Some(custom) = custom_path {
            let p = PathBuf::from(custom.trim());
            if p.exists() {
                return Some(p);
            }
        }

        // Check local virtualenv in working dir if provided
        if let Some(dir) = working_dir {
            let venv_candidates = [".venv", "venv", "env", ".env"];
            for venv_name in &venv_candidates {
                let venv_dir = dir.join(venv_name);
                #[cfg(windows)]
                let candidate = venv_dir.join("Scripts").join("python.exe");
                #[cfg(not(windows))]
                let candidate = venv_dir.join("bin").join("python");

                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        // Probe system candidates
        #[cfg(windows)]
        let system_candidates = ["python.exe", "python3.exe", "py.exe", "python"];
        #[cfg(not(windows))]
        let system_candidates = ["python3", "python"];

        for candidate in &system_candidates {
            if Self::verify_executable(candidate) {
                return Some(PathBuf::from(*candidate));
            }
        }

        None
    }

    /// Tests if a command exists and runs cleanly.
    fn verify_executable(executable: &str) -> bool {
        let mut cmd = Command::new(executable);
        cmd.arg("--version");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        match cmd.status() {
            Ok(status) => status.success(),
            Err(_) => false,
        }
    }

    /// Queries the version string of the selected interpreter (e.g. "Python 3.14.6").
    pub fn query_version(interpreter: &Path) -> Option<String> {
        let mut cmd = Command::new(interpreter);
        cmd.arg("--version");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Ok(output) = cmd.output() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !stdout.is_empty() {
                return Some(stdout);
            }
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if !stderr.is_empty() {
                return Some(stderr);
            }
        }
        None
    }
}

#[cfg(windows)]
pub fn kill_process_by_pid(pid: u32) {
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
        fn TerminateProcess(hProcess: isize, uExitCode: u32) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
    }
    const PROCESS_TERMINATE: u32 = 0x0001;
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle != 0 && handle != -1 {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
    }
}

#[cfg(not(windows))]
pub fn kill_process_by_pid(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
}

pub struct PythonRunnerState {
    pub is_drawer_open: bool,
    pub status: PythonRunStatus,
    pub output_lines: Vec<OutputLine>,
    pub max_output_lines: usize,
    pub preview_line_limit: usize,
    pub spool_path: Option<PathBuf>,
    pub total_stdout_lines: usize,
    pub total_stdout_bytes: u64,
    pub is_truncated_in_preview: bool,
    pub custom_python_path: Option<String>,
    pub cached_interpreter_info: Option<String>,
    pub auto_scroll_to_bottom: bool,
    pub word_wrap: bool,
    pub active_pid: Option<u32>,
    rx: Option<Receiver<OutputMessage>>,
    cancel_flag: Option<Arc<AtomicBool>>,
}

impl Default for PythonRunnerState {
    fn default() -> Self {
        Self {
            is_drawer_open: false,
            status: PythonRunStatus::Idle,
            output_lines: Vec::new(),
            max_output_lines: 1_000,
            preview_line_limit: 1_000,
            spool_path: None,
            total_stdout_lines: 0,
            total_stdout_bytes: 0,
            is_truncated_in_preview: false,
            custom_python_path: None,
            cached_interpreter_info: None,
            auto_scroll_to_bottom: true,
            word_wrap: true,
            active_pid: None,
            rx: None,
            cancel_flag: None,
        }
    }
}

impl PythonRunnerState {
    pub fn is_running(&self) -> bool {
        matches!(self.status, PythonRunStatus::Running { .. })
    }

    pub fn clear_output(&mut self) {
        self.output_lines.clear();
        self.total_stdout_lines = 0;
        self.total_stdout_bytes = 0;
        self.is_truncated_in_preview = false;
        if !self.is_running() {
            self.status = PythonRunStatus::Idle;
        }
    }

    /// Spawns the Python process and streams the given code bytes via stdin.
    pub fn run(
        &mut self,
        code_bytes: Vec<u8>,
        working_dir: Option<PathBuf>,
        env_vars: Vec<(String, String)>,
    ) {
        if self.is_running() {
            return;
        }

        self.is_drawer_open = true;
        self.output_lines.clear();

        // 1. Discover Python interpreter
        let interpreter = match PythonEnvironment::find_interpreter(
            self.custom_python_path.as_deref(),
            working_dir.as_deref(),
        ) {
            Some(p) => p,
            None => {
                let err_msg = "Python interpreter was not found on system PATH.\nPlease install Python or configure its path in Settings.".to_string();
                self.output_lines.push(OutputLine {
                    kind: OutputChunkKind::Stderr,
                    text: err_msg.clone(),
                    timestamp: Instant::now(),
                });
                self.status = PythonRunStatus::FailedToStart(err_msg);
                return;
            }
        };

        // Fetch or cache interpreter version label
        let ver_str = PythonEnvironment::query_version(&interpreter)
            .unwrap_or_else(|| interpreter.to_string_lossy().to_string());
        self.cached_interpreter_info = Some(ver_str.clone());

        // 2. Prepare Command with stdin piped and windows console suppression
        let mut cmd = Command::new(&interpreter);
        cmd.arg("-u").arg("-"); // Unbuffered, read script from stdin

        if let Some(ref dir) = working_dir {
            if dir.exists() {
                cmd.current_dir(dir);
            }
        }

        cmd.env("PYTHONIOENCODING", "utf-8");
        cmd.env("PYTHONUNBUFFERED", "1");

        for (k, v) in &env_vars {
            cmd.env(k, v);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Failed to spawn Python process: {}", e);
                self.output_lines.push(OutputLine {
                    kind: OutputChunkKind::Stderr,
                    text: err_msg.clone(),
                    timestamp: Instant::now(),
                });
                self.status = PythonRunStatus::FailedToStart(err_msg);
                return;
            }
        };

        let pid = child.id();
        let started_at = Instant::now();
        self.status = PythonRunStatus::Running { started_at, pid };

        // Create ephemeral spool file for full fidelity output (O(1) memory)
        static SPOOL_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
        let spool_idx = SPOOL_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_dir = std::env::temp_dir().join("UltraViewer");
        let _ = std::fs::create_dir_all(&temp_dir);
        let spool_path = temp_dir.join(format!("py_spool_{}_{}_{}.txt", std::process::id(), pid, spool_idx));
        self.spool_path = Some(spool_path.clone());

        // Banner info line with target data file and cwd
        let target_display = env_vars
            .iter()
            .find(|(k, _)| k == "ULTRAVIEWER_FILE_NAME")
            .map(|(_, name)| format!(" [target: {}]", name))
            .unwrap_or_default();
        let dir_display = working_dir
            .as_ref()
            .map(|p| format!(" [cwd: {}]", p.display()))
            .unwrap_or_default();
        self.output_lines.push(OutputLine {
            kind: OutputChunkKind::System,
            text: format!(">>> Executing with {} (via stdin){}{}", ver_str, target_display, dir_display),
            timestamp: started_at,
        });

        let mut child_stdin = child.stdin.take();
        let child_stdout = child.stdout.take();
        let child_stderr = child.stderr.take();

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag_clone = Arc::clone(&cancel_flag);

        let (tx, rx) = crossbeam_channel::unbounded();

        self.rx = Some(rx);
        self.cancel_flag = Some(cancel_flag);
        self.active_pid = Some(pid);

        // Worker thread 1: Stream code into child stdin then close it
        std::thread::Builder::new()
            .name("py-stdin-writer".to_string())
            .spawn(move || {
                if let Some(mut stdin) = child_stdin.take() {
                    let _ = stdin.write_all(&code_bytes);
                    let _ = stdin.flush();
                    drop(stdin); // Signals EOF to Python stdin
                }
            })
            .ok();

        // Worker thread 2: Read stdout lines, stream to disk spool file, and send preview
        let tx_stdout = tx.clone();
        let spool_path_thread = spool_path.clone();
        let preview_limit = self.preview_line_limit;

        if let Some(stdout) = child_stdout {
            std::thread::Builder::new()
                .name("py-stdout-reader".to_string())
                .spawn(move || {
                    let mut spool_file = std::fs::File::create(&spool_path_thread).ok().map(std::io::BufWriter::new);
                    let reader = BufReader::new(stdout);
                    let mut stdout_count: usize = 0;
                    let mut stdout_bytes: u64 = 0;

                    for line_res in reader.lines() {
                        match line_res {
                            Ok(line) => {
                                stdout_count += 1;
                                stdout_bytes += line.len() as u64 + 1; // + newline

                                if let Some(ref mut writer) = spool_file {
                                    let _ = writeln!(writer, "{}", line);
                                }

                                if stdout_count <= preview_limit {
                                    let _ = tx_stdout.send(OutputMessage::Line(OutputLine {
                                        kind: OutputChunkKind::Stdout,
                                        text: line,
                                        timestamp: Instant::now(),
                                    }));
                                } else if stdout_count == preview_limit + 1 {
                                    let _ = tx_stdout.send(OutputMessage::TruncationReached { preview_limit });
                                }
                            }
                            Err(_) => break,
                        }
                    }

                    if let Some(mut writer) = spool_file {
                        let _ = writer.flush();
                    }

                    let _ = tx_stdout.send(OutputMessage::StdoutFinished {
                        total_lines: stdout_count,
                        total_bytes: stdout_bytes,
                    });
                })
                .ok();
        }

        // Worker thread 3: Read stderr lines
        let tx_stderr = tx.clone();
        if let Some(stderr) = child_stderr {
            std::thread::Builder::new()
                .name("py-stderr-reader".to_string())
                .spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line_res in reader.lines() {
                        match line_res {
                            Ok(line) => {
                                let _ = tx_stderr.send(OutputMessage::Line(OutputLine {
                                    kind: OutputChunkKind::Stderr,
                                    text: line,
                                    timestamp: Instant::now(),
                                }));
                            }
                            Err(_) => break,
                        }
                    }
                })
                .ok();
        }

        // Worker thread 4: Wait for child exit and report completion
        std::thread::Builder::new()
            .name("py-waiter".to_string())
            .spawn(move || {
                let status_res = child.wait().ok();

                let duration = started_at.elapsed();
                let was_cancelled = cancel_flag_clone.load(Ordering::SeqCst);

                if was_cancelled {
                    let _ = tx.send(OutputMessage::Terminated { duration });
                } else if let Some(exit_status) = status_res {
                    let exit_code = exit_status.code();
                    let success = exit_status.success();
                    let _ = tx.send(OutputMessage::Finished {
                        exit_code,
                        duration,
                        success,
                    });
                } else {
                    let _ = tx.send(OutputMessage::Terminated { duration });
                }
            })
            .ok();
    }

    /// Kills the running Python process immediately.
    pub fn stop(&mut self) {
        if let Some(ref flag) = self.cancel_flag {
            flag.store(true, Ordering::SeqCst);
        }

        if let Some(pid) = self.active_pid.take() {
            kill_process_by_pid(pid);
        }
    }

    /// Polls updates from the background worker channel.
    /// Returns true if any updates were processed.
    pub fn poll(&mut self) -> bool {
        let mut updated = false;
        let mut should_cleanup = false;

        if let Some(ref rx) = self.rx {
            while let Ok(msg) = rx.try_recv() {
                updated = true;
                match msg {
                    OutputMessage::Line(line) => {
                        if self.output_lines.len() >= self.max_output_lines {
                            self.output_lines.remove(0);
                        }
                        self.output_lines.push(line);
                    }
                    OutputMessage::TruncationReached { preview_limit } => {
                        self.is_truncated_in_preview = true;
                        self.output_lines.push(OutputLine {
                            kind: OutputChunkKind::System,
                            text: format!("... [Output preview limited to first {} lines. Use 'Save As' or 'Open in New Tab' for full output] ...", preview_limit),
                            timestamp: Instant::now(),
                        });
                    }
                    OutputMessage::StdoutFinished { total_lines, total_bytes } => {
                        self.total_stdout_lines = total_lines;
                        self.total_stdout_bytes = total_bytes;
                        if total_lines > self.preview_line_limit {
                            self.is_truncated_in_preview = true;
                        }
                    }
                    OutputMessage::Finished {
                        exit_code,
                        duration,
                        success,
                    } => {
                        let summary = match exit_code {
                            Some(code) if code == 0 => {
                                format!("=== Execution finished in {:.2}s (exit code 0) ===", duration.as_secs_f64())
                            }
                            Some(code) => {
                                format!("=== Execution failed in {:.2}s (exit code {}) ===", duration.as_secs_f64(), code)
                            }
                            None => format!("=== Execution ended in {:.2}s ===", duration.as_secs_f64()),
                        };
                        self.output_lines.push(OutputLine {
                            kind: if success { OutputChunkKind::System } else { OutputChunkKind::Stderr },
                            text: summary,
                            timestamp: Instant::now(),
                        });
                        self.status = PythonRunStatus::Finished {
                            exit_code,
                            duration,
                            success,
                        };
                        should_cleanup = true;
                    }
                    OutputMessage::Terminated { duration } => {
                        self.output_lines.push(OutputLine {
                            kind: OutputChunkKind::System,
                            text: format!("=== Process terminated by user after {:.2}s ===", duration.as_secs_f64()),
                            timestamp: Instant::now(),
                        });
                        self.status = PythonRunStatus::Terminated { duration };
                        should_cleanup = true;
                    }
                    OutputMessage::FailedToStart(err) => {
                        self.status = PythonRunStatus::FailedToStart(err);
                        should_cleanup = true;
                    }
                }
            }
        }

        if should_cleanup {
            self.rx = None;
            self.active_pid = None;
            self.cancel_flag = None;
        }

        updated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_interpreter_detects_python() {
        let interpreter = PythonEnvironment::find_interpreter(None, None);
        assert!(interpreter.is_some(), "Expected Python interpreter to be found on system");
    }

    #[test]
    fn test_python_run_simple_stdin_calculation() {
        let interpreter = PythonEnvironment::find_interpreter(None, None);
        if interpreter.is_none() {
            return;
        }

        let mut runner = PythonRunnerState::default();
        let code = b"print('HELLO_ULTRAVIEWER')\nprint(12 * 12)\n";
        runner.run(code.to_vec(), None, Vec::new());

        assert!(runner.is_running());
        let start = Instant::now();
        while runner.is_running() && start.elapsed() < Duration::from_secs(5) {
            runner.poll();
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(!runner.is_running());
        match runner.status {
            PythonRunStatus::Finished { success, .. } => assert!(success),
            _ => panic!("Expected process to finish successfully"),
        }

        let all_stdout: Vec<_> = runner
            .output_lines
            .iter()
            .filter(|l| l.kind == OutputChunkKind::Stdout)
            .map(|l| l.text.as_str())
            .collect();

        assert!(all_stdout.contains(&"HELLO_ULTRAVIEWER"));
        assert!(all_stdout.contains(&"144"));
    }

    #[test]
    fn test_python_run_captures_stderr_traceback() {
        let interpreter = PythonEnvironment::find_interpreter(None, None);
        if interpreter.is_none() {
            return;
        }

        let mut runner = PythonRunnerState::default();
        let code = b"1 / 0\n";
        runner.run(code.to_vec(), None, Vec::new());

        let start = Instant::now();
        while runner.is_running() && start.elapsed() < Duration::from_secs(5) {
            runner.poll();
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(!runner.is_running());
        match runner.status {
            PythonRunStatus::Finished { success, exit_code, .. } => {
                assert!(!success);
                assert_eq!(exit_code, Some(1));
            }
            _ => panic!("Expected process to finish with failure"),
        }

        let has_div_zero = runner
            .output_lines
            .iter()
            .filter(|l| l.kind == OutputChunkKind::Stderr)
            .any(|l| l.text.contains("ZeroDivisionError"));
        assert!(has_div_zero, "Expected stderr to capture ZeroDivisionError traceback");
    }

    #[test]
    fn test_python_stop_terminates_infinite_loop() {
        let interpreter = PythonEnvironment::find_interpreter(None, None);
        if interpreter.is_none() {
            return;
        }

        let mut runner = PythonRunnerState::default();
        let code = b"import time\nwhile True:\n    time.sleep(0.1)\n";
        runner.run(code.to_vec(), None, Vec::new());

        assert!(runner.is_running());
        std::thread::sleep(Duration::from_millis(150));
        runner.stop();

        let start = Instant::now();
        while runner.is_running() && start.elapsed() < Duration::from_secs(5) {
            runner.poll();
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(!runner.is_running());
        assert!(matches!(runner.status, PythonRunStatus::Terminated { .. }));
    }

    #[test]
    fn test_python_runner_data_interop_env_vars() {
        let interpreter = PythonEnvironment::find_interpreter(None, None);
        if interpreter.is_none() {
            return;
        }

        let mut runner = PythonRunnerState::default();
        let code = b"import os\nprint(f\"INTEROP_PATH={os.environ.get('ULTRAVIEWER_FILE_PATH')}\")\nprint(f\"INTEROP_TYPE={os.environ.get('ULTRAVIEWER_FILE_TYPE')}\")\n";
        let env_vars = vec![
            ("ULTRAVIEWER_FILE_PATH".to_string(), "C:\\data\\test_feed.xml".to_string()),
            ("ULTRAVIEWER_FILE_NAME".to_string(), "test_feed.xml".to_string()),
            ("ULTRAVIEWER_FILE_TYPE".to_string(), "xml".to_string()),
        ];
        runner.run(code.to_vec(), None, env_vars);

        let start = Instant::now();
        while runner.is_running() && start.elapsed() < Duration::from_secs(5) {
            runner.poll();
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(!runner.is_running());
        let all_stdout: Vec<_> = runner
            .output_lines
            .iter()
            .filter(|l| l.kind == OutputChunkKind::Stdout)
            .map(|l| l.text.as_str())
            .collect();

        assert!(all_stdout.iter().any(|s| s.contains("INTEROP_PATH=C:\\data\\test_feed.xml")));
        assert!(all_stdout.iter().any(|s| s.contains("INTEROP_TYPE=xml")));
    }

    #[test]
    fn test_python_runner_preview_truncation_and_spool_fidelity() {
        let interpreter = PythonEnvironment::find_interpreter(None, None);
        if interpreter.is_none() {
            return;
        }

        let mut runner = PythonRunnerState::default();
        runner.preview_line_limit = 100;
        runner.max_output_lines = 100;

        let code = b"for i in range(250):\n    print(f'LINE_{i}')\n";
        runner.run(code.to_vec(), None, Vec::new());

        let start = Instant::now();
        while runner.is_running() && start.elapsed() < Duration::from_secs(5) {
            runner.poll();
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(!runner.is_running());
        assert!(runner.is_truncated_in_preview, "Expected is_truncated_in_preview to be true");
        assert_eq!(runner.total_stdout_lines, 250);

        // Preview should have capped stdout lines
        let stdout_lines = runner.output_lines.iter().filter(|l| l.kind == OutputChunkKind::Stdout).count();
        assert_eq!(stdout_lines, 100);

        // Complete spool file on disk must have all 250 lines!
        let spool_path = runner.spool_path.expect("Spool file must exist");
        assert!(spool_path.exists());
        let spool_content = std::fs::read_to_string(&spool_path).unwrap();
        let spooled_lines: Vec<&str> = spool_content.lines().collect();
        assert_eq!(spooled_lines.len(), 250);
        assert_eq!(spooled_lines[0], "LINE_0");
        assert_eq!(spooled_lines[249], "LINE_249");

        let _ = std::fs::remove_file(&spool_path);
    }
}
