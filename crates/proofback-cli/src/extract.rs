//! PDF parsing in an isolated child process (plan section 10).
//!
//! The parent runs `proofback __extract <pdf>`; the child lowers its own resource
//! limits, parses the PDF and prints [`proofback_pdf::Extracted`] as JSON. The parent
//! enforces a wall-clock timeout and kills the child if it is exceeded.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use proofback_pdf::{Extracted, Limits};
use proofback_store::{Error, LimitsConfig, Result};

/// Memory ceiling for the child process (address space), in bytes.
const CHILD_MEMORY: u64 = 4 << 30;

/// Parse a PDF in a child process.
pub fn extract_isolated(pdf: &Path, limits: &LimitsConfig) -> Result<Extracted> {
    let exe = std::env::current_exe()
        .map_err(|e| Error::Input(format!("cannot locate the proofback executable: {e}")))?;
    let mut child = Command::new(exe)
        .arg("__extract")
        .arg(pdf)
        .arg(limits.max_pdf_mb.to_string())
        .arg(limits.max_pages.to_string())
        .arg(limits.max_annotations.to_string())
        .arg(limits.parse_timeout_secs.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Input(format!("cannot start the PDF parser: {e}")))?;
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let out_thread = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stdout.read_to_end(&mut b);
        b
    });
    let err_thread = std::thread::spawn(move || {
        let mut b = String::new();
        let _ = stderr.read_to_string(&mut b);
        b
    });
    let deadline = Instant::now() + Duration::from_secs(limits.parse_timeout_secs.max(1));
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Input(format!(
                    "parsing {} took longer than {} s and was stopped",
                    pdf.display(),
                    limits.parse_timeout_secs
                )));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(Error::Input(format!("PDF parser failed: {e}"))),
        }
    };
    let out = out_thread.join().unwrap_or_default();
    let err = err_thread.join().unwrap_or_default();
    if !status.success() {
        let msg = err.trim();
        return Err(Error::Input(if msg.is_empty() {
            format!(
                "the PDF parser stopped unexpectedly ({status}); the file may be damaged or too large"
            )
        } else {
            msg.to_owned()
        }));
    }
    serde_json::from_slice(&out)
        .map_err(|e| Error::Input(format!("PDF parser returned invalid data: {e}")))
}

/// Entry point of the child process. Returns the process exit code.
pub fn child_main(args: &[String]) -> i32 {
    let [pdf, max_mb, max_pages, max_annots, timeout] = args else {
        eprintln!("internal: bad __extract arguments");
        return 2;
    };
    let num = |s: &String| s.parse::<u64>().unwrap_or(0);
    lower_limits(num(timeout));
    let limits = Limits {
        max_bytes: num(max_mb).saturating_mul(1024 * 1024),
        max_pages: num(max_pages) as usize,
        max_annotations: num(max_annots) as usize,
    };
    let bytes = match std::fs::metadata(pdf) {
        Ok(m) if m.len() > limits.max_bytes => {
            eprintln!("{pdf} is larger than the {} MB limit", num(max_mb));
            return 3;
        }
        Ok(_) => match std::fs::read(pdf) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("cannot read {pdf}: {e}");
                return 3;
            }
        },
        Err(e) => {
            eprintln!("cannot read {pdf}: {e}");
            return 3;
        }
    };
    let pdfium = match proofback_pdf::load_pdfium(None) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 3;
        }
    };
    match proofback_pdf::extract(&pdfium, &bytes, limits) {
        Ok(x) => {
            let json = serde_json::to_string(&x).expect("serialisable");
            println!("{json}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            3
        }
    }
}

#[cfg(unix)]
fn lower_limits(cpu_secs: u64) {
    use rlimit::Resource;
    // Best effort: some platforms do not enforce every limit.
    let _ = Resource::AS.set(CHILD_MEMORY, CHILD_MEMORY);
    let cpu = cpu_secs.max(1);
    let _ = Resource::CPU.set(cpu, cpu);
    let _ = Resource::CORE.set(0, 0);
}

#[cfg(not(unix))]
fn lower_limits(_cpu_secs: u64) {
    let _ = CHILD_MEMORY;
}
