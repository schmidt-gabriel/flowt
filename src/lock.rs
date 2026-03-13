use std::io::Write;
use std::path::PathBuf;

fn lock_file_path() -> PathBuf {
    let lock_path = std::env::var("FLOWT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|mut home| {
                    home.push(".flowt");
                    home
                })
                .unwrap_or_else(|| PathBuf::from("."))
        });
    lock_path.join("service.lock")
}

/// Write (or remove) the engine lock file with the current process PID.
pub fn set_engine_status(is_running: bool) -> anyhow::Result<()> {
    let path = lock_file_path();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    if is_running {
        let pid = std::process::id();
        std::fs::write(&path, format!("{}", pid))?;
    } else {
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}

/// Returns true if the engine lock file exists and the recorded process is alive.
pub fn is_engine_running() -> bool {
    let path = lock_file_path();
    if let Ok(pid_str) = std::fs::read_to_string(&path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            return is_process_running(pid);
        }
    }
    false
}

/// Atomically try to claim the engine lock for the current process.
/// Returns true only if this process successfully created the lock file
/// (i.e. no other process got there first).
pub fn try_claim_engine() -> bool {
    let path = lock_file_path();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // create_new(true) fails atomically if the file already exists
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut f) => {
            let pid = std::process::id();
            let _ = f.write_all(format!("{}", pid).as_bytes());
            true
        }
        Err(_) => false,
    }
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        let output = Command::new("kill").args(["-0", &pid.to_string()]).output();
        match output {
            Ok(result) => result.status.success(),
            Err(_) => false,
        }
    }

    #[cfg(windows)]
    {
        use std::process::Command;
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV"])
            .output();
        match output {
            Ok(result) => {
                let output_str = String::from_utf8_lossy(&result.stdout);
                output_str.lines().count() > 1
            }
            Err(_) => false,
        }
    }
}
