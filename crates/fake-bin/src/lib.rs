//! A fake program for a test to run in place of a real one, such as `gh` — DEV-1356.
//!
//! **Every test that fakes a program goes through [`FakeBin`]**, and
//! `every_fake_program_in_the_workspace_is_made_here` below fails if a test anywhere in
//! `crates/` writes a script of its own.
//!
//! # Why a test cannot just write a script and run it
//!
//! Linux refuses to execute a file that any process holds open for writing, with `ETXTBSY`
//! ("Text file busy"). A test that writes a script and runs it straight away closes its own
//! descriptor first, but `cargo test` runs tests on parallel threads. If another thread forks
//! while the script is open, the child gets a copy of the descriptor and keeps it until that
//! child executes its own program. Close-on-exec does not help inside that window. So a
//! script that is run straight after it is written can be busy, at random. That is what
//! turned #89 red on 21 November 2026.
//!
//! Writing under another name and renaming does not help either: the copied descriptor
//! belongs to the inode, and a rename keeps the inode.
//!
//! # Why this one cannot
//!
//! The two files a fake involves are never both written by a test *and* executed:
//!
//! | File | Written by | Executed |
//! | -- | -- | -- |
//! | `<dir>/<name>`, a symlink to `crates/fake-bin/launcher` | git, at checkout | yes |
//! | `<dir>/<name>.sh`, the body | the test | no: `/bin/sh` reads it |
//!
//! The launcher's last line is `exec /bin/sh "$0.sh" "$@"`, so the only programs the kernel
//! is asked to execute are the launcher and `/bin/sh`. No test opens either for writing,
//! and creating a symlink opens nothing. Without a writer there is nothing to be busy.
//!
//! The process the caller started stays the one running the body, because `exec` keeps the
//! process id. That is why a caller that kills the fake at a deadline still kills the right
//! process.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// The checked-in launcher every fake links to. It is executable in git (mode 100755).
pub const LAUNCHER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/launcher");

/// A program called `name`, alone in a temporary directory of its own, that runs `body` as
/// a `/bin/sh` script. The directory goes when this does.
///
/// Put [`dir`](Self::dir) on `PATH` to stand in for a program the code under test finds
/// there, or run [`path`](Self::path) directly. The directory is the test's to use: a fake
/// that records its calls or reads canned answers can keep them beside itself.
#[derive(Debug)]
pub struct FakeBin {
    dir: TempDir,
    name: String,
}

impl FakeBin {
    /// `body` is shell source. It needs no `#!` line, because it is never executed itself.
    pub fn new(name: &str, body: &str) -> Self {
        let dir = TempDir::new().expect("a directory for the fake program");
        let fake = FakeBin {
            dir,
            name: name.to_string(),
        };
        // The body first, so the program never exists without one.
        std::fs::write(fake.body(), body).expect("write the fake program's body");
        std::os::unix::fs::symlink(LAUNCHER, fake.path()).expect("link the fake program");
        fake
    }

    /// The directory the program is in, alone: what goes on `PATH`.
    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// The program itself.
    pub fn path(&self) -> PathBuf {
        self.dir.path().join(&self.name)
    }

    /// The script the program runs. Rewriting it between runs is safe: it is only read.
    pub fn body(&self) -> PathBuf {
        self.dir.path().join(format!("{}.sh", self.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    #[test]
    fn the_launcher_is_executable_as_checked_in() {
        let mode = std::fs::metadata(LAUNCHER).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "launcher mode is {mode:o}");
    }

    #[test]
    fn a_fake_gets_its_arguments_and_input_and_its_exit_status_comes_back() {
        let fake = FakeBin::new("gh", "echo \"args: $*\"\n/bin/cat\necho oops >&2\nexit 3\n");
        let mut child = Command::new(fake.path())
            .args(["api", "a b"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"sent\n").unwrap();
        let output = child.wait_with_output().unwrap();

        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "args: api a b\nsent\n"
        );
        assert_eq!(String::from_utf8_lossy(&output.stderr), "oops\n");
        assert_eq!(output.status.code(), Some(3));
    }

    #[test]
    fn a_fake_is_found_on_path_by_its_name() {
        let fake = FakeBin::new("gh", "echo found\n");
        let output = Command::new("gh").env("PATH", fake.dir()).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "found\n");
    }

    /// The process the caller started is the one running the body, so killing it kills
    /// the fake — what `review-desk-harvest`'s deadline test depends on.
    #[test]
    fn the_process_started_is_the_process_running_the_body() {
        let fake = FakeBin::new("gh", "echo $$\n");
        let child = Command::new(fake.path())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            pid.to_string()
        );
    }

    /// Fakes made and run on many threads at once, while other threads fork, as `cargo
    /// test` does. Not the proof — the module documentation is — but the shape that used
    /// to fail, kept running so that a change to `new` that reopens the window shows.
    #[test]
    fn fakes_made_and_run_while_other_threads_fork_are_never_busy() {
        let threads: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..40 {
                        let fake = FakeBin::new("fake", "exit 0\n");
                        let status = Command::new(fake.path())
                            .status()
                            .unwrap_or_else(|error| panic!("{error}"));
                        assert!(status.success());
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
    }

    /// Every `.rs` file under `crates/` other than this crate's. A test that writes a
    /// script of its own names an interpreter with `#!/`, which a body here does not need.
    #[test]
    fn every_fake_program_in_the_workspace_is_made_here() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let own = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut offenders = Vec::new();
        let mut pending = vec![crates.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.starts_with(own) || path.ends_with("target") {
                    continue;
                }
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    if text.contains("#!/") || text.contains("from_mode(0o7") {
                        offenders.push(path.display().to_string());
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "these write a program of their own; use review_desk_fake_bin::FakeBin: {offenders:?}"
        );
    }
}
