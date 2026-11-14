#[cfg(windows)]
pub(super) use nt::{Disposition, open_file_beneath};

pub(super) fn is_plain_name(name: &str) -> bool {
    let forbidden = |c: char| c < ' ' || r#"<>:"/\|?*"#.contains(c);

    !matches!(name, "" | "." | "..")
        && !is_device_name(name)
        && !name.ends_with(['.', ' '])
        && !name.contains(forbidden)
}

pub(super) fn is_device_name(name: &str) -> bool {
    let stem = name.split_once('.').map_or(name, |(stem, _)| stem);
    let stem = stem.trim_end_matches(' ');
    let (prefix, rest) = stem.split_at_checked(3).unwrap_or_default();
    let is = |device: &str| prefix.eq_ignore_ascii_case(device);

    let mut rest = rest.chars();
    match (rest.next(), rest.next()) {
        (None, _) => is("CON") || is("PRN") || is("AUX") || is("NUL"),
        (Some('0'..='9' | '¹' | '²' | '³'), None) => is("COM") || is("LPT"),
        _ => false,
    }
}

#[cfg(windows)]
mod nt {
    use super::is_plain_name;
    use std::{
        fs::File,
        io,
        os::windows::io::{AsRawHandle, FromRawHandle},
        ptr,
    };
    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_CREATE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF, FILE_OVERWRITE,
                FILE_OVERWRITE_IF, FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
            },
        },
        Win32::{
            Foundation::{
                GENERIC_READ, GENERIC_WRITE, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE,
                RtlNtStatusToDosError, UNICODE_STRING,
            },
            Storage::FileSystem::{
                FILE_ATTRIBUTE_NORMAL, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
                SYNCHRONIZE,
            },
            System::IO::IO_STATUS_BLOCK,
        },
    };

    #[derive(Debug, Clone, Copy)]
    pub enum Disposition {
        Open,
        OpenOrCreate,
        CreateNew,
        Truncate,
        TruncateOrCreate,
    }

    /// Opens the file at `path` in `dir` with a single system call. Fails if
    /// `path` isn't plain names joined by slashes, if anything on the way is a
    /// symlink, junction or other reparse point, or if it's a directory, so
    /// it never leaves `dir`.
    pub fn open_file_beneath(
        dir: &File,
        path: &str,
        read: bool,
        write: bool,
        disposition: Disposition,
    ) -> io::Result<File> {
        let mut wide = Vec::with_capacity(path.len());
        for part in path.split(['/', '\\']) {
            if !is_plain_name(part) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            if !wide.is_empty() {
                wide.push(u16::from(b'\\'));
            }
            wide.extend(part.encode_utf16());
        }
        let len = u16::try_from(wide.len() * 2)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let name = UNICODE_STRING {
            Length: len,
            MaximumLength: len,
            Buffer: wide.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: dir.as_raw_handle(),
            ObjectName: &name,
            Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
            SecurityDescriptor: ptr::null(),
            SecurityQualityOfService: ptr::null(),
        };

        let mut access = SYNCHRONIZE | FILE_READ_ATTRIBUTES;
        if read {
            access |= GENERIC_READ;
        }
        if write {
            access |= GENERIC_WRITE;
        }
        let disposition = match disposition {
            Disposition::Open => FILE_OPEN,
            Disposition::OpenOrCreate => FILE_OPEN_IF,
            Disposition::CreateNew => FILE_CREATE,
            Disposition::Truncate => FILE_OVERWRITE,
            Disposition::TruncateOrCreate => FILE_OVERWRITE_IF,
        };

        let mut handle = ptr::null_mut();
        let mut status_block = IO_STATUS_BLOCK::default();
        // SAFETY: the pointers are to locals that outlive the call and `dir`
        // stays open during it.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                access,
                &attributes,
                &mut status_block,
                ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                // So an open file can't be removed or renamed.
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                disposition,
                FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
                ptr::null(),
                0,
            )
        };
        if status < 0 {
            let code = unsafe { RtlNtStatusToDosError(status) };
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        // SAFETY: the call succeeded so `handle` is an open handle nothing else owns.
        Ok(unsafe { File::from_raw_handle(handle) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn check_plain(name: &str, expected: bool) {
        assert_eq!(is_plain_name(name), expected, "{name:?}");
    }

    #[track_caller]
    fn check_device(name: &str, expected: bool) {
        assert_eq!(is_device_name(name), expected, "{name:?}");
    }

    #[test]
    fn ordinary_names_are_plain() {
        for name in [
            "a",
            "file.txt",
            "archive.tar.gz",
            ".hidden",
            " leading space",
            "inner space",
            "a..b",
            "console.txt",
            "名前",
        ] {
            check_plain(name, true);
        }
    }

    #[test]
    fn other_names_are_not_plain() {
        for name in [
            "",
            ".",
            "..",
            "...",
            "dot.",
            "space ",
            "both. ",
            "CON",
            "nul.txt",
            "com1 .log",
        ] {
            check_plain(name, false);
        }
    }

    #[test]
    fn reserved_characters_make_a_name_not_plain() {
        let reserved: Vec<char> = ('\0'..' ').chain(r#"<>:"/\|?*"#.chars()).collect();

        for c in '\0'..='\x7f' {
            check_plain(&format!("a{c}b"), !reserved.contains(&c));
        }
    }

    #[test]
    fn device_names_match_in_any_case_and_with_any_extension() {
        let numbers = ('0'..='9').chain(['¹', '²', '³']);
        let numbered = numbers.flat_map(|n| [format!("COM{n}"), format!("LPT{n}")]);
        let devices = ["CON", "PRN", "AUX", "NUL"].map(String::from).into_iter();

        for device in devices.chain(numbered) {
            check_device(&device, true);
            check_device(&device.to_lowercase(), true);
            check_device(&format!("{device}.txt"), true);
            check_device(&format!("{device}.tar.gz"), true);
            check_device(&format!("{device} .txt"), true);
        }
    }

    #[test]
    fn other_names_are_not_devices() {
        for name in [
            "",
            ".",
            "..",
            ".con",
            "x.con",
            "console",
            "auxiliary",
            "nul_",
            "CO",
            "COM",
            "COM10",
            "LPT01",
            "COMa",
            "COM¹¹",
            "名前",
        ] {
            check_device(name, false);
        }
    }
}
