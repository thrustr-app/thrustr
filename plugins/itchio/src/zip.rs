//! Reads the uncompressed size of a remote zip from its central directory.

use std::ops::Range;

/// Large enough to (usually) hold the whole central directory.
const TAIL_LEN: u64 = 256 * 1024;
const MAX_DIRECTORY_LEN: u64 = 64 * 1024 * 1024;

const DIRECTORY_END_SIGNATURE: u32 = 0x0605_4b50;
const DIRECTORY_END_LEN: usize = 22;
const ZIP64_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
const ZIP64_LOCATOR_LEN: usize = 20;
const ZIP64_DIRECTORY_END_SIGNATURE: u32 = 0x0606_4b50;
const DIRECTORY_ENTRY_SIGNATURE: u32 = 0x0201_4b50;
const DIRECTORY_ENTRY_LEN: usize = 46;
const ZIP64_EXTRA_ID: u16 = 0x0001;

/// `None` if the file isn't a zip.
pub async fn extracted_size<E>(
    mut fetch: impl AsyncFnMut(&str) -> Result<(Vec<u8>, u64), E>,
) -> Result<Option<u64>, E> {
    let (tail, file_len) = fetch(&format!("-{TAIL_LEN}")).await?;
    let tail_start = file_len.saturating_sub(tail.len() as u64);

    let Some(range) = locate_directory(&tail, tail_start) else {
        return Ok(None);
    };
    if range.end - range.start > MAX_DIRECTORY_LEN {
        return Ok(None);
    }

    if let Some(start) = range.start.checked_sub(tail_start) {
        let directory = tail.get(start as usize..(range.end - tail_start) as usize);
        return Ok(directory.and_then(total_uncompressed_size));
    }

    let range = format!("{}-{}", range.start, range.end - 1);
    let (directory, _) = fetch(&range).await?;
    Ok(total_uncompressed_size(&directory))
}

/// Byte range of the central directory. The start is derived from where the
/// directory ends, so zips with prepended data work too.
fn locate_directory(tail: &[u8], tail_start: u64) -> Option<Range<u64>> {
    let directory_end = (0..=tail.len().checked_sub(DIRECTORY_END_LEN)?)
        .rev()
        .find(|&at| {
            read_u32(tail, at) == Some(DIRECTORY_END_SIGNATURE)
                && read_u16(tail, at + 20)
                    .is_some_and(|comment| at + DIRECTORY_END_LEN + comment as usize == tail.len())
        })?;

    let entries = read_u16(tail, directory_end + 10)?;
    let len = read_u32(tail, directory_end + 12)?;
    let offset = read_u32(tail, directory_end + 16)?;
    if entries != u16::MAX && len != u32::MAX && offset != u32::MAX {
        let end = tail_start + directory_end as u64;
        return Some(end.checked_sub(len.into())?..end);
    }

    let locator = directory_end.checked_sub(ZIP64_LOCATOR_LEN)?;
    if read_u32(tail, locator)? != ZIP64_LOCATOR_SIGNATURE {
        return None;
    }
    let zip64_end = usize::try_from(read_u64(tail, locator + 8)?.checked_sub(tail_start)?).ok()?;
    if read_u32(tail, zip64_end)? != ZIP64_DIRECTORY_END_SIGNATURE {
        return None;
    }
    let len = read_u64(tail, zip64_end + 40)?;
    let end = tail_start + zip64_end as u64;
    Some(end.checked_sub(len)?..end)
}

fn total_uncompressed_size(directory: &[u8]) -> Option<u64> {
    let mut total = 0u64;
    let mut at = 0;

    while at < directory.len() {
        if read_u32(directory, at)? != DIRECTORY_ENTRY_SIGNATURE {
            return None;
        }
        let uncompressed = read_u32(directory, at + 24)?;
        let name_len = read_u16(directory, at + 28)? as usize;
        let extra_len = read_u16(directory, at + 30)? as usize;
        let comment_len = read_u16(directory, at + 32)? as usize;

        let extra_start = at + DIRECTORY_ENTRY_LEN + name_len;
        let extra = directory.get(extra_start..extra_start + extra_len)?;
        let next = extra_start + extra_len + comment_len;
        if next > directory.len() {
            return None;
        }

        let size = if uncompressed == u32::MAX {
            zip64_uncompressed_size(extra)?
        } else {
            uncompressed.into()
        };
        total = total.checked_add(size)?;
        at = next;
    }

    Some(total)
}

fn zip64_uncompressed_size(mut extra: &[u8]) -> Option<u64> {
    while extra.len() >= 4 {
        let id = read_u16(extra, 0)?;
        let len = read_u16(extra, 2)? as usize;
        if id == ZIP64_EXTRA_ID {
            return read_u64(extra, 4);
        }
        extra = extra.get(4 + len..)?;
    }
    None
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(read(bytes, at)?))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(read(bytes, at)?))
}

fn read_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(read(bytes, at)?))
}

fn read<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
    bytes.get(at..at.checked_add(N)?)?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use std::convert::Infallible;

    const DATA_LEN: usize = 1000;

    struct ZipBuilder {
        sizes: Vec<u64>,
        zip64: bool,
        comment: Vec<u8>,
        prefix: Vec<u8>,
    }

    fn zip(sizes: &[u64]) -> ZipBuilder {
        ZipBuilder {
            sizes: sizes.to_vec(),
            zip64: false,
            comment: Vec::new(),
            prefix: Vec::new(),
        }
    }

    impl ZipBuilder {
        fn zip64(mut self) -> Self {
            self.zip64 = true;
            self
        }

        fn comment(mut self, comment: &[u8]) -> Self {
            self.comment = comment.to_vec();
            self
        }

        fn prepend(mut self, prefix: &[u8]) -> Self {
            self.prefix = prefix.to_vec();
            self
        }

        fn build(self) -> Vec<u8> {
            let mut out = vec![0; DATA_LEN];
            for (index, &size) in self.sizes.iter().enumerate() {
                let name = format!("file{index}");
                let mut extra = Vec::new();
                // An unrelated field to skip.
                extra.extend(0x5455u16.to_le_bytes());
                extra.extend(5u16.to_le_bytes());
                extra.extend([0; 5]);
                let narrow = u32::try_from(size).ok().filter(|&size| size != u32::MAX);
                if narrow.is_none() {
                    extra.extend(ZIP64_EXTRA_ID.to_le_bytes());
                    extra.extend(16u16.to_le_bytes());
                    extra.extend(size.to_le_bytes());
                    extra.extend(size.to_le_bytes());
                }

                out.extend(DIRECTORY_ENTRY_SIGNATURE.to_le_bytes());
                out.extend([0; 16]);
                out.extend(narrow.unwrap_or(u32::MAX).to_le_bytes());
                out.extend(narrow.unwrap_or(u32::MAX).to_le_bytes());
                out.extend((name.len() as u16).to_le_bytes());
                out.extend((extra.len() as u16).to_le_bytes());
                out.extend([0; 14]);
                out.extend(name.as_bytes());
                out.extend(extra);
            }

            let directory_len = (out.len() - DATA_LEN) as u64;
            let entries = self.sizes.len() as u64;

            if self.zip64 {
                let zip64_end = out.len() as u64;
                out.extend(ZIP64_DIRECTORY_END_SIGNATURE.to_le_bytes());
                out.extend(44u64.to_le_bytes());
                out.extend([0; 12]);
                out.extend(entries.to_le_bytes());
                out.extend(entries.to_le_bytes());
                out.extend(directory_len.to_le_bytes());
                out.extend((DATA_LEN as u64).to_le_bytes());

                out.extend(ZIP64_LOCATOR_SIGNATURE.to_le_bytes());
                out.extend(0u32.to_le_bytes());
                out.extend(zip64_end.to_le_bytes());
                out.extend(1u32.to_le_bytes());
            }

            let (entries, directory_len, offset) = if self.zip64 {
                (u16::MAX, u32::MAX, u32::MAX)
            } else {
                (entries as u16, directory_len as u32, DATA_LEN as u32)
            };
            out.extend(DIRECTORY_END_SIGNATURE.to_le_bytes());
            out.extend([0; 4]);
            out.extend(entries.to_le_bytes());
            out.extend(entries.to_le_bytes());
            out.extend(directory_len.to_le_bytes());
            out.extend(offset.to_le_bytes());
            out.extend((self.comment.len() as u16).to_le_bytes());
            out.extend(self.comment);

            [self.prefix, out].concat()
        }
    }

    fn large_directory() -> Vec<u64> {
        vec![1; TAIL_LEN as usize / 32]
    }

    #[track_caller]
    fn check(file: &[u8], expected_size: Option<u64>, expected_requests: usize) {
        let mut requests = 0;
        let size = block_on(extracted_size(async |range: &str| {
            requests += 1;
            Ok::<_, Infallible>((serve(file, range), file.len() as u64))
        }));
        assert_eq!(
            (size.unwrap(), requests),
            (expected_size, expected_requests)
        );
    }

    fn serve(file: &[u8], range: &str) -> Vec<u8> {
        let (start, end) = range.split_once('-').expect("a range");
        let range = match (start.parse::<usize>().ok(), end.parse::<usize>()) {
            (None, Ok(suffix)) => file.len().saturating_sub(suffix)..file.len(),
            (Some(start), Ok(end)) => start..(end + 1).min(file.len()),
            _ => panic!("unexpected range `{range}`"),
        };
        file[range].to_vec()
    }

    #[test]
    fn sums_the_size_of_every_entry() {
        check(&zip(&[10, 20, 30]).build(), Some(60), 1);
        check(&zip(&[]).build(), Some(0), 1);
    }

    #[test]
    fn finds_the_directory_past_a_comment() {
        let file = zip(&[10, 20]).comment(b"PK\x05\x06 not the end").build();
        check(&file, Some(30), 1);
    }

    #[test]
    fn reads_zip64_sizes() {
        let huge = 5 * 1024 * 1024 * 1024;
        check(&zip(&[huge, 7]).zip64().build(), Some(huge + 7), 1);
    }

    #[test]
    fn reads_zips_with_prepended_data() {
        let file = zip(&[10, 20]).prepend(&[0x7f; 4096]).build();
        check(&file, Some(30), 1);
    }

    #[test]
    fn fetches_a_large_directory_separately() {
        let sizes = large_directory();
        let total = sizes.len() as u64;
        check(&zip(&sizes).build(), Some(total), 2);
        check(&zip(&sizes).zip64().build(), Some(total), 2);
        check(&zip(&sizes).prepend(&[0x7f; 4096]).build(), Some(total), 2);
    }

    #[test]
    fn has_no_size_for_other_files() {
        check(b"MZ not a zip", None, 1);
        check(b"", None, 1);
    }

    #[test]
    fn has_no_size_for_a_corrupt_directory() {
        let mut file = zip(&[10, 20]).build();
        file[DATA_LEN] = 0;
        check(&file, None, 1);
    }
}
