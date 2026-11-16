use std::{fs::File, io};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    Offset(u64),
    End,
}

impl At {
    pub fn after(self, len: usize) -> At {
        match self {
            At::Offset(offset) => At::Offset(offset + len as u64),
            At::End => At::End,
        }
    }

    /// Checked before queuing so a write past the largest offset fails alone
    /// instead of failing the queue, and `after` can't overflow.
    pub fn check_fits(self, len: usize) -> io::Result<()> {
        match self {
            At::Offset(offset) if offset > i64::MAX as u64 - len as u64 => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "write past the largest file offset",
            )),
            _ => Ok(()),
        }
    }
}

#[cfg(unix)]
pub fn write_at(file: &File, at: At, data: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    match at {
        At::Offset(offset) => file.write_all_at(data, offset),
        At::End => append(file, data),
    }
}

#[cfg(target_os = "linux")]
fn append(file: &File, mut data: &[u8]) -> io::Result<()> {
    use rustix::io::{Errno, ReadWriteFlags, pwritev2};
    while !data.is_empty() {
        match pwritev2(file, &[io::IoSlice::new(data)], 0, ReadWriteFlags::APPEND) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => data = &data[written..],
            Err(Errno::INTR) => {}
            Err(Errno::NOSYS | Errno::NOTSUP) => return append_in_append_mode(file, data),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "linux")))]
fn append(file: &File, data: &[u8]) -> io::Result<()> {
    append_in_append_mode(file, data)
}

/// Appends by switching the file to append mode for the duration of the write.
/// This is safe because only one thread ever writes to a given open file.
#[cfg(unix)]
fn append_in_append_mode(file: &File, data: &[u8]) -> io::Result<()> {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    use std::io::Write;

    let flags = fcntl_getfl(file)?;
    fcntl_setfl(file, flags | OFlags::APPEND)?;
    let written = (&mut &*file).write_all(data);
    fcntl_setfl(file, flags)?;
    written
}

#[cfg(windows)]
pub fn write_at(file: &File, mut at: At, mut data: &[u8]) -> io::Result<()> {
    use std::os::windows::fs::FileExt;

    while !data.is_empty() {
        let offset = match at {
            At::Offset(offset) => offset,
            // Windows writes at the end of the file at this offset.
            At::End => u64::MAX,
        };
        match file.seek_write(data, offset)? {
            0 => return Err(io::ErrorKind::WriteZero.into()),
            written => {
                data = &data[written..];
                at = at.after(written);
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    loop {
        match file.read_at(buf, offset) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}

#[cfg(windows)]
pub fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buf, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::host::fs::tests::open_rw;
    use std::fs;
    use tempfile::TempDir;

    /// The fallback for kernels without `RWF_APPEND` and the only way elsewhere.
    #[cfg(unix)]
    #[test]
    fn appending_in_append_mode_leaves_positioned_writes_in_place() {
        let dir = TempDir::new().unwrap();
        let file = open_rw(&dir.path().join("log"));
        append_in_append_mode(&file, b"abc").unwrap();
        append_in_append_mode(&file, b"de").unwrap();
        write_at(&file, At::Offset(0), b"X").unwrap();
        assert_eq!(fs::read(dir.path().join("log")).unwrap(), b"Xbcde");
    }

    #[test]
    fn appends_from_separate_calls_never_overwrite_each_other() {
        const APPENDS: usize = 5000;
        const LEN: usize = 16;
        const HEAD: &[u8] = b"head";
        let dir = TempDir::new().unwrap();
        let log = dir.path().join("log");
        fs::write(&log, HEAD).unwrap();

        std::thread::scope(|scope| {
            for byte in *b"ab" {
                let file = open_rw(&log);
                scope.spawn(move || {
                    for _ in 0..APPENDS {
                        write_at(&file, At::End, &[byte; LEN]).unwrap();
                    }
                });
            }
        });

        let log = fs::read(log).unwrap();
        let (head, appended) = log.split_at(HEAD.len());
        assert_eq!(head, HEAD);
        assert_eq!(appended.len(), 2 * APPENDS * LEN);
        for chunk in appended.chunks(LEN) {
            assert!(chunk.iter().all(|&b| b == chunk[0]), "torn append");
        }
    }
}
