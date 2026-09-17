//! The operator's document converter: a command that turns a slide
//! deck, a Word file or a spreadsheet into a PDF, so the model reads a
//! document the way it reads any other. The server knows nothing about
//! which program that is; `FLASH_DOC_CONVERTER` is a command template
//! with an `{input}` and an `{outdir}` placeholder, the input is written
//! to a directory of its own, the command runs under a deadline, and
//! the one PDF it left behind is what goes on. No converter, no
//! documents: the upload is refused with a sentence.
//!
//! The operator owns the program the template names, its sandboxing
//! and its patching; the server only bounds how long it may run and
//! how large its answer may be.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// How often a running conversion is looked in on.
const POLL: Duration = Duration::from_millis(50);
/// Most of the program's stderr kept for the journal.
const STDERR_EXCERPT: usize = 8 * 1024;

/// Why a conversion produced no PDF. Display is for the journal;
/// `user_message` is the sentence a person sees.
#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("could not start the converter: {0}")]
    Spawn(std::io::Error),
    #[error("the converter took too long")]
    Timeout,
    #[error("the converter exited with {status}: {stderr}")]
    Failed { status: String, stderr: String },
    #[error("the converter produced no PDF")]
    NoOutput,
    #[error("the converter's output is not a PDF")]
    NotPdf,
    #[error("the converter's output is too large")]
    TooLarge,
    #[error("scratch space: {0}")]
    Io(std::io::Error),
}

impl ConvertError {
    pub fn user_message(&self) -> &'static str {
        match self {
            ConvertError::Timeout => {
                "Converting that document took too long. Try a smaller \
                                      one, or save it as a PDF."
            }
            ConvertError::TooLarge => {
                "That document is too large once converted. Save part of it as a PDF."
            }
            _ => {
                "That document could not be converted to a PDF. Save it as a PDF and \
                  upload that."
            }
        }
    }
}

/// The template, parsed once at boot.
pub struct DocConverter {
    argv: Vec<String>,
    timeout: Duration,
    /// One conversion at a time: an office suite is a heavy process,
    /// and two of them at once is how a small host runs out of memory.
    lock: Mutex<()>,
    serial: AtomicU64,
}

impl std::fmt::Debug for DocConverter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocConverter")
            .field("program", &self.argv.first())
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl DocConverter {
    /// Splits the template on whitespace. `{input}` and `{outdir}` must
    /// each appear somewhere in it (on their own or inside a word, as in
    /// `--outdir={outdir}`); Err is the sentence the boot log prints.
    pub fn parse(template: &str, timeout: Duration) -> Result<Self, String> {
        let argv: Vec<String> = template.split_whitespace().map(str::to_string).collect();
        if argv.is_empty() {
            return Err("FLASH_DOC_CONVERTER is empty".to_string());
        }
        for placeholder in ["{input}", "{outdir}"] {
            if !argv[1..].iter().any(|a| a.contains(placeholder)) {
                return Err(format!(
                    "FLASH_DOC_CONVERTER must contain {placeholder}, for example: \
                     soffice --headless --convert-to pdf --outdir {{outdir}} {{input}}"
                ));
            }
        }
        Ok(Self {
            argv,
            timeout,
            lock: Mutex::new(()),
            serial: AtomicU64::new(0),
        })
    }

    /// The program the template names, for the boot log.
    pub fn program(&self) -> &str {
        &self.argv[0]
    }

    /// Converts `bytes`, written as `input.<extension>` in a fresh
    /// directory under `work_dir`, into the PDF the command leaves in
    /// that directory. The directory is removed whatever happened.
    /// `max_bytes` bounds the PDF that is read back.
    pub fn to_pdf(
        &self,
        work_dir: &Path,
        extension: &str,
        bytes: &[u8],
        max_bytes: u64,
    ) -> Result<Vec<u8>, ConvertError> {
        let _one_at_a_time = self.lock.lock();
        let dir = self.scratch(work_dir)?;
        let outcome = self.run(&dir, extension, bytes, max_bytes);
        let _ = std::fs::remove_dir_all(&dir);
        outcome
    }

    fn scratch(&self, work_dir: &Path) -> Result<PathBuf, ConvertError> {
        let n = self.serial.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = work_dir.join(format!("{}-{nanos}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(ConvertError::Io)?;
        Ok(dir)
    }

    fn run(
        &self,
        dir: &Path,
        extension: &str,
        bytes: &[u8],
        max_bytes: u64,
    ) -> Result<Vec<u8>, ConvertError> {
        // The extension comes from the classifier's table, never from
        // the person's own name for the file.
        let input = dir.join(format!("input.{extension}"));
        std::fs::write(&input, bytes).map_err(ConvertError::Io)?;
        let input_path = input.to_string_lossy().into_owned();
        let dir_path = dir.to_string_lossy().into_owned();
        let args: Vec<String> = self.argv[1..]
            .iter()
            .map(|a| {
                a.replace("{input}", &input_path)
                    .replace("{outdir}", &dir_path)
            })
            .collect();
        let mut child = Command::new(&self.argv[0])
            .args(&args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(ConvertError::Spawn)?;
        // stderr is drained from the start, on a thread of its own: a
        // program that writes more than the pipe holds would otherwise
        // block on the write until the deadline, with the excerpt read
        // only after it exits. The thread keeps the first
        // `STDERR_EXCERPT` bytes and discards the rest. On a timeout it
        // is left to end by itself, once every process holding the
        // pipe's write end has gone.
        let drain = child.stderr.take().map(|mut pipe| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = pipe
                    .by_ref()
                    .take(STDERR_EXCERPT as u64)
                    .read_to_end(&mut buf);
                let _ = std::io::copy(&mut pipe, &mut std::io::sink());
                buf
            })
        });
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ConvertError::Timeout);
                }
                Ok(None) => std::thread::sleep(POLL),
                Err(e) => return Err(ConvertError::Io(e)),
            }
        };
        let stderr = drain
            .and_then(|t| t.join().ok())
            .map(|buf| String::from_utf8_lossy(&buf).into_owned())
            .unwrap_or_default();
        if !status.success() {
            return Err(ConvertError::Failed {
                status: status.to_string(),
                stderr,
            });
        }
        let pdf = std::fs::read_dir(dir)
            .map_err(ConvertError::Io)?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .find(|path| {
                path.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
            })
            .ok_or(ConvertError::NoOutput)?;
        let size = std::fs::metadata(&pdf).map_err(ConvertError::Io)?.len();
        if size > max_bytes {
            return Err(ConvertError::TooLarge);
        }
        let out = std::fs::read(&pdf).map_err(ConvertError::Io)?;
        if !out.starts_with(b"%PDF-") {
            return Err(ConvertError::NotPdf);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("flash-convert-{tag}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// A converter that copies the input to `out.pdf`. Perl is on every
    /// machine the tests run on, and its one-liners have no spaces the
    /// template's split would break on.
    fn copier() -> String {
        "perl -MFile::Copy -e copy($ARGV[0],$ARGV[1]) {input} {outdir}/out.pdf".to_string()
    }

    /// A converter that sleeps past any short deadline.
    fn sleeper() -> String {
        "perl -e sleep(5) {input} {outdir}".to_string()
    }

    #[test]
    fn the_template_needs_both_placeholders() {
        assert!(DocConverter::parse("", Duration::from_secs(1)).is_err());
        let err = DocConverter::parse("soffice --convert-to pdf {input}", Duration::from_secs(1))
            .unwrap_err();
        assert!(err.contains("{outdir}"), "{err}");
        let err = DocConverter::parse("{input} {outdir}", Duration::from_secs(1)).unwrap_err();
        assert!(
            err.contains("{input}"),
            "the program itself is not a placeholder: {err}"
        );
        let inside =
            DocConverter::parse("soffice --outdir={outdir} {input}", Duration::from_secs(1))
                .unwrap();
        assert_eq!(inside.program(), "soffice");
        let ok = DocConverter::parse(
            "soffice --headless --convert-to pdf --outdir {outdir} {input}",
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(ok.program(), "soffice");
    }

    #[test]
    fn a_converter_that_writes_a_pdf_hands_it_back_and_cleans_up() {
        let work = temp_dir("ok");
        let converter = DocConverter::parse(&copier(), Duration::from_secs(20)).unwrap();
        let out = converter
            .to_pdf(&work, "docx", b"%PDF-1.4 pretend", 1 << 20)
            .unwrap();
        assert_eq!(out, b"%PDF-1.4 pretend");
        assert!(
            std::fs::read_dir(&work).unwrap().next().is_none(),
            "the scratch directory was removed"
        );
        // Output that is not a PDF is refused, and so is one past the cap.
        assert!(matches!(
            converter.to_pdf(&work, "docx", b"just words", 1 << 20),
            Err(ConvertError::NotPdf)
        ));
        assert!(matches!(
            converter.to_pdf(&work, "docx", b"%PDF-1.4 too long", 4),
            Err(ConvertError::TooLarge)
        ));
    }

    #[test]
    fn a_converter_past_its_deadline_is_killed() {
        let work = temp_dir("slow");
        let converter = DocConverter::parse(&sleeper(), Duration::from_millis(200)).unwrap();
        let started = Instant::now();
        let err = converter.to_pdf(&work, "docx", b"x", 1 << 20).unwrap_err();
        assert!(matches!(err, ConvertError::Timeout), "{err}");
        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(
            err.user_message(),
            "Converting that document took too long. Try a smaller one, or save it as a PDF."
        );
    }

    /// A converter that writes far more to stderr than a pipe holds
    /// (256 KiB against a 64 KiB Linux buffer), then fails.
    fn chatterbox() -> String {
        "perl -e print{*STDERR}((\"e\"x4096,\"\\n\")x64);exit(3) {input} {outdir}".to_string()
    }

    #[test]
    fn a_converter_that_floods_stderr_still_exits_and_is_reported() {
        let work = temp_dir("chatty");
        let converter = DocConverter::parse(&chatterbox(), Duration::from_secs(5)).unwrap();
        let started = Instant::now();
        let err = converter.to_pdf(&work, "docx", b"x", 1 << 20).unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "the program blocked on its stderr until the deadline"
        );
        match err {
            ConvertError::Failed { status, stderr } => {
                assert!(status.contains('3'), "{status}");
                assert_eq!(stderr.len(), STDERR_EXCERPT);
                assert!(stderr.starts_with("eeee"));
            }
            other => panic!("{other}"),
        }
    }

    #[test]
    fn a_program_that_does_not_exist_is_a_spawn_error() {
        let work = temp_dir("missing");
        let converter = DocConverter::parse(
            "flash-no-such-converter-binary {input} {outdir}",
            Duration::from_secs(1),
        )
        .unwrap();
        let err = converter.to_pdf(&work, "docx", b"x", 1 << 20).unwrap_err();
        assert!(matches!(err, ConvertError::Spawn(_)), "{err}");
        assert!(err.user_message().contains("Save it as a PDF"));
    }
}
