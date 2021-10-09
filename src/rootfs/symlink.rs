use crate::syscall::{syscall::create_syscall, Syscall};
use anyhow::{bail, Context, Result};
use std::fs::remove_file;
use std::path::Path;

pub struct Symlink {
    syscall: Box<dyn Syscall>,
}

impl Default for Symlink {
    fn default() -> Self {
        Self::new()
    }
}

impl Symlink {
    pub fn new() -> Symlink {
        Symlink {
            syscall: create_syscall(),
        }
    }

    // Create symlinks for subsystems that have been comounted e.g. cpu -> cpu,cpuacct, cpuacct -> cpu,cpuacct
    pub fn setup_comount_symlinks(&self, cgroup_root: &Path, subsystem_name: &str) -> Result<()> {
        if !subsystem_name.contains(',') {
            return Ok(());
        }

        for comount in subsystem_name.split_terminator(',') {
            let link = cgroup_root.join(comount);
            self.syscall
                .symlink(Path::new(subsystem_name), &link)
                .with_context(|| format!("failed to symlink {:?} to {:?}", link, subsystem_name))?;
        }

        Ok(())
    }

    pub fn setup_ptmx(&self, rootfs: &Path) -> Result<()> {
        let ptmx = rootfs.join("dev/ptmx");
        if let Err(e) = remove_file(&ptmx) {
            if e.kind() != ::std::io::ErrorKind::NotFound {
                bail!("could not delete /dev/ptmx")
            }
        }

        self.syscall
            .symlink(Path::new("pts/ptmx"), &ptmx)
            .context("failed to symlink ptmx")?;
        Ok(())
    }

    // separating kcore symlink out from setup_default_symlinks for a better way to do the unit test,
    // since not every architecture has /proc/kcore file.
    pub fn setup_kcore_symlink(&self, rootfs: &Path) -> Result<()> {
        if Path::new("/proc/kcore").exists() {
            self.syscall
                .symlink(Path::new("/proc/kcore"), &rootfs.join("dev/kcore"))
                .context("Failed to symlink kcore")?;
        }
        Ok(())
    }

    pub fn setup_default_symlinks(&self, rootfs: &Path) -> Result<()> {
        let defaults = [
            ("/proc/self/fd", "dev/fd"),
            ("/proc/self/fd/0", "dev/stdin"),
            ("/proc/self/fd/1", "dev/stdout"),
            ("/proc/self/fd/2", "dev/stderr"),
        ];
        for (src, dst) in defaults {
            self.syscall
                .symlink(Path::new(src), &rootfs.join(dst))
                .context("failed to symlink defaults")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syscall::test::TestHelperSyscall;
    use crate::utils::TempDir;
    use nix::{
        fcntl::{open, OFlag},
        sys::stat::Mode,
    };
    use std::path::PathBuf;

    #[test]
    fn test_setup_ptmx() {
        {
            let tmp_dir = TempDir::new("/tmp/test_setup_ptmx").unwrap();
            let symlink = Symlink::new();
            assert!(symlink.setup_ptmx(tmp_dir.path()).is_ok());
            let want = (PathBuf::from("pts/ptmx"), tmp_dir.path().join("dev/ptmx"));
            let got = &symlink
                .syscall
                .as_any()
                .downcast_ref::<TestHelperSyscall>()
                .unwrap()
                .get_symlink_args()[0];
            assert_eq!(want, *got)
        }
        // make remove_file goes into the bail! path
        {
            let tmp_dir = TempDir::new("/tmp/test_setup_ptmx").unwrap();
            open(
                &tmp_dir.path().join("dev"),
                OFlag::O_RDWR | OFlag::O_CREAT,
                Mode::from_bits_truncate(0o644),
            )
            .unwrap();

            let symlink = Symlink::new();
            assert!(symlink.setup_ptmx(tmp_dir.path()).is_err());
            assert_eq!(
                0,
                symlink
                    .syscall
                    .as_any()
                    .downcast_ref::<TestHelperSyscall>()
                    .unwrap()
                    .get_symlink_args()
                    .len()
            );
        }
    }

    #[test]
    fn test_setup_default_symlinks() {
        let tmp_dir = TempDir::new("/tmp/test_setup_default_symlinks").unwrap();
        let symlink = Symlink::new();
        assert!(symlink.setup_default_symlinks(tmp_dir.path()).is_ok());
        let want = vec![
            (
                PathBuf::from("/proc/self/fd"),
                tmp_dir.path().join("dev/fd"),
            ),
            (
                PathBuf::from("/proc/self/fd/0"),
                tmp_dir.path().join("dev/stdin"),
            ),
            (
                PathBuf::from("/proc/self/fd/1"),
                tmp_dir.path().join("dev/stdout"),
            ),
            (
                PathBuf::from("/proc/self/fd/2"),
                tmp_dir.path().join("dev/stderr"),
            ),
        ];
        let got = symlink
            .syscall
            .as_any()
            .downcast_ref::<TestHelperSyscall>()
            .unwrap()
            .get_symlink_args();
        assert_eq!(want, got)
    }

    #[test]
    fn test_setup_comount_symlinks() {
        let tmp_dir = TempDir::new("/tmp/test_setup_default_symlinks").unwrap();
        let symlink = Symlink::new();
        assert!(symlink
            .setup_comount_symlinks(tmp_dir.path(), "cpu,cpuacct")
            .is_ok());
        let want = vec![
            (PathBuf::from("cpu,cpuacct"), tmp_dir.path().join("cpu")),
            (PathBuf::from("cpu,cpuacct"), tmp_dir.path().join("cpuacct")),
        ];
        let got = symlink
            .syscall
            .as_any()
            .downcast_ref::<TestHelperSyscall>()
            .unwrap()
            .get_symlink_args();
        assert_eq!(want, got)
    }
}
