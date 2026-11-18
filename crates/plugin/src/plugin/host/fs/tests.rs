use super::{
    writer::{MAX_MERGED, MAX_QUEUED_BYTES, MAX_QUEUED_WRITES},
    *,
};
use crate::plugin::state::DATA_MOUNT;
use domain::download::{DownloadPhase, DownloadReporter};
use futures::FutureExt;
use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    future::poll_fn,
    mem,
    path::{Path, PathBuf},
    pin::pin,
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
};
use tempfile::TempDir;
use wasmtime::{
    Config, Engine, Store,
    component::{FutureConsumer, Lift, ResourceTable},
};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxView, WasiView};

async fn check_writes(writes: &[(At, &str)], expected: &str) {
    let h = Harness::new(&[]);
    let probe = h.probe();
    let file = probe.file("game.bin");
    for &(at, data) in writes {
        probe.push(&file, at, data).unwrap();
    }

    probe.writer.drain().await.unwrap();
    assert_eq!(probe.read("game.bin"), expected.as_bytes(), "{writes:?}");
    let len: usize = writes.iter().map(|(_, data)| data.len()).sum();
    assert_eq!(probe.disk.landed(), len as u64, "{writes:?}");
}

#[tokio::test]
async fn queued_writes_land_in_order() {
    use At::*;
    check_writes(&[(Offset(0), "aaaa"), (Offset(1), "bb")], "abba").await;
    check_writes(&[(Offset(2), "cd"), (Offset(0), "ab")], "abcd").await;
    check_writes(&[(End, "ab"), (End, "cd")], "abcd").await;
    check_writes(&[(Offset(0), "ab"), (End, "cd"), (Offset(1), "x")], "axcd").await;
    check_writes(&[(Offset(3), "x")], "\0\0\0x").await;
}

/// Queues `(file, at, len)` writes to two files while a held write keeps the
/// writer busy, then checks the length of each write that reaches the disk.
async fn check_merged(writes: &[(usize, At, usize)], expected: &[usize]) {
    let h = Harness::new(&[]);
    let probe = h.probe();
    let files = [probe.file("a"), probe.file("b")];
    let paused = probe.hold_a_write(&probe.file("held"));
    for &(file, at, len) in writes {
        probe.push(&files[file], at, vec![0; len]).unwrap();
    }

    drop(paused);
    probe.writer.drain().await.unwrap();
    let expected: Vec<_> = expected.iter().map(|&len| len as u64).collect();
    assert_eq!(probe.disk.writes()[1..], expected, "{writes:?}");
}

#[tokio::test]
async fn continuous_writes_are_merged() {
    use At::*;
    check_merged(&[(0, Offset(0), 10), (0, Offset(10), 5)], &[15]).await;
    check_merged(&[(0, End, 10), (0, End, 5)], &[15]).await;
    let bytes = [(0, Offset(0), 1), (0, Offset(1), 1), (0, Offset(2), 1)];
    check_merged(&bytes, &[3]).await;
}

#[tokio::test]
async fn writes_elsewhere_are_not_merged() {
    use At::*;
    let max = MAX_MERGED as u64;
    let separate: &[&[(usize, At, usize)]] = &[
        &[(0, Offset(0), 10), (0, Offset(20), 5)],
        &[(0, Offset(0), 10), (1, Offset(10), 5)],
        &[(0, Offset(0), 10), (0, End, 5)],
        &[(0, Offset(10), 5), (0, Offset(0), 10)],
        &[(0, Offset(0), MAX_MERGED), (0, Offset(max), 1)],
    ];
    for &writes in separate {
        let lens: Vec<_> = writes.iter().map(|&(_, _, len)| len).collect();
        check_merged(writes, &lens).await;
    }
}

#[tokio::test]
async fn a_cancelled_call_waits_only_for_the_write_in_flight() {
    let h = Harness::new(&[]);
    let probe = h.probe();
    let file = probe.file("game.bin");
    let paused = probe.disk.pause();
    for at in [0, 10, 20, 30] {
        probe.push(&file, At::Offset(at), "abc").unwrap();
    }
    probe.disk.wait_until_holding();
    let closed = Arc::downgrade(&file);
    drop(file);

    probe.writer.abandon();
    assert_eq!(probe.queued_bytes(), 3, "only the write in flight is left");
    let mut drain = pin!(probe.writer.drain());
    assert!(drain.as_mut().now_or_never().is_none());
    drop(paused);
    drain.await.unwrap();
    assert!(
        closed.upgrade().is_none(),
        "the writer still holds the file"
    );
    assert_eq!(probe.read("game.bin"), b"abc");
}

/// Fills the queue with writes of `lens` while the disk is paused, then checks
/// how much of a large write a stream still accepts.
async fn check_room(lens: &[usize], expected: usize) {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    let context = format!(
        "{} writes of {} bytes",
        lens.len(),
        lens.iter().sum::<usize>()
    );
    h.run(async |fs, roots| {
        let game = fs.open(&roots.download, "game.bin", OpenFlags::empty());
        let game = game.await.unwrap();
        let file = fs.file(&game);
        let _paused = probe.disk.pause();
        let mut at = 0;
        for &len in lens {
            probe.push(&file, At::Offset(at), vec![0; len]).unwrap();
            // Leaves a gap so no writes merge.
            at += len as u64 + 1;
        }

        let mut output = fs.output(&game, Some(at)).unwrap();
        assert_eq!(output.is_ready(), expected > 0, "{context}");
        let written = output.write(&vec![0; MAX_WRITE + 1]).unwrap();
        assert_eq!(written, expected, "{context}");
    })
    .await;

    probe.writer.drain().await.unwrap();
    let landed = lens.iter().sum::<usize>() + expected;
    assert_eq!(probe.disk.landed(), landed as u64, "{context}");
}

#[tokio::test]
async fn a_stream_writes_only_what_the_queue_has_room_for() {
    check_room(&[], MAX_WRITE).await;
    check_room(&[MAX_QUEUED_BYTES - 10], 10).await;
    check_room(&[MAX_QUEUED_BYTES], 0).await;
    check_room(&[1; MAX_QUEUED_WRITES - 1], MAX_WRITE).await;
    check_room(&[1; MAX_QUEUED_WRITES + 1], 0).await;
}

#[tokio::test]
async fn streams_together_never_queue_more_than_the_limit() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    let full = MAX_QUEUED_BYTES / MAX_WRITE;
    h.run(async |fs, roots| {
        let game = fs.open(&roots.download, "game.bin", OpenFlags::empty());
        let game = game.await.unwrap();
        let mut outputs: Vec<_> = (0..full * 2)
            .map(|i| fs.output(&game, Some((i * MAX_WRITE) as u64)).unwrap())
            .collect();
        let _paused = probe.disk.pause();

        let chunk = vec![b'x'; MAX_WRITE];
        for (i, output) in outputs.iter_mut().enumerate() {
            let expected = if i < full { MAX_WRITE } else { 0 };
            assert_eq!(output.write(&chunk).unwrap(), expected, "stream {i}");
        }
    })
    .await;

    probe.writer.drain().await.unwrap();
    assert_eq!(probe.disk.landed(), MAX_QUEUED_BYTES as u64);
}

#[tokio::test]
async fn waiting_streams_hold_no_room() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let game = fs.open(&roots.download, "game.bin", OpenFlags::empty());
        let game = game.await.unwrap();
        let _paused = probe.disk.pause();
        let mut first = fs.output(&game, Some(0)).unwrap();
        while first.write(&vec![b'x'; MAX_WRITE]).unwrap() > 0 {}
        let queued = probe.queued_bytes();

        let mut waiting = fs.output(&game, Some(0)).unwrap();
        assert_eq!(waiting.write(b"abc").unwrap(), 0);
        assert_eq!(waiting.cancel(b"abc"), None, "a cancel takes nothing");
        drop(waiting);
        assert_eq!(probe.queued_bytes(), queued, "waiting took room");
    })
    .await;

    probe.writer.drain().await.unwrap();
    assert_eq!(probe.queued_bytes(), 0);
}

#[tokio::test]
async fn idle_streams_hold_no_room() {
    let mut h = Harness::new(&[]);
    let probe = h.probe();
    // Enough streams to fill the queue if each held a full write's room.
    let count = MAX_QUEUED_BYTES / MAX_WRITE * 2;
    h.run(async |fs, roots| {
        let file = fs.open(&roots.download, "game.bin", OpenFlags::CREATE);
        let file = file.await.unwrap();
        let _paused = probe.disk.pause();
        let mut outputs = Vec::new();
        for i in 0..count {
            let mut output = fs.output(&file, Some(i as u64 * 20)).unwrap();
            assert_eq!(output.write(b"0123456789").unwrap(), 10, "stream {i}");
            outputs.push(output);
        }
        for (i, output) in outputs.iter_mut().enumerate() {
            assert_eq!(output.write(b"abcdefghij").unwrap(), 10, "stream {i}");
        }
    })
    .await;

    probe.writer.drain().await.unwrap();
    let expected = b"0123456789abcdefghij".repeat(count);
    assert_eq!(probe.read("game.bin"), expected);
}

#[tokio::test]
async fn a_stream_is_ready_once_the_queue_has_room() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let game = fs.open(&roots.download, "game.bin", OpenFlags::empty());
        let game = game.await.unwrap();
        let paused = probe.disk.pause();
        let mut first = fs.output(&game, Some(0)).unwrap();
        let mut last = fs.output(&game, Some(0)).unwrap();
        assert!(last.is_ready());
        while first.write(&vec![b'x'; MAX_WRITE]).unwrap() > 0 {}
        assert!(!last.is_ready());

        drop(paused);
        probe.writer.drain().await.unwrap();
        assert!(last.is_ready());
    })
    .await;
}

#[tokio::test]
async fn writes_past_the_largest_offset_fail_alone() {
    let mut h = Harness::new(&[]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        for root in [&roots.download, &roots.data] {
            let file = fs.open(root, "game.bin", OpenFlags::CREATE).await.unwrap();
            for at in [i64::MAX as u64, u64::MAX - 1, u64::MAX] {
                let write = fs.write(&file, Some(at), "ab").await;
                check_error(write, &ErrorCode::Invalid, format!("at {at}"));
            }
            fs.write(&file, Some(0), "ab").await.unwrap();
        }
    })
    .await;

    probe.writer.drain().await.unwrap();
    assert_eq!(probe.read("game.bin"), b"ab");
    assert_eq!(probe.read_data("game.bin"), b"ab");
}

#[tokio::test]
async fn writing_to_a_read_only_descriptor_fails_alone() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let root = &roots.download;
        let open = fs.open_with(root, "game.bin", OpenFlags::empty(), DescriptorFlags::READ);
        let read_only = open.await.unwrap();

        for at in [Some(0), None] {
            let write = fs.write(&read_only, at, "x").await;
            check_error(write, &ErrorCode::BadDescriptor, format!("at {at:?}"));
        }
        let stream = fs.output(&read_only, Some(0)).map(drop);
        check_error(stream, &ErrorCode::BadDescriptor, "stream");

        let game = fs.open(root, "game.bin", OpenFlags::empty()).await.unwrap();
        fs.write(&game, Some(0), "x").await.unwrap();
    })
    .await;

    probe.writer.drain().await.unwrap();
    assert_eq!(probe.read("game.bin"), b"xame.bin");
}

#[tokio::test]
async fn a_failed_write_fails_every_later_call_with_its_error() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    let read_only = File::open(probe.download.join("game.bin")).unwrap();
    let expected = ErrorCode::from(write_at(&read_only, At::Offset(0), b"x").unwrap_err());
    h.run(async |fs, roots| {
        let root = &roots.download;
        let game = fs.open(root, "game.bin", OpenFlags::empty()).await.unwrap();
        // Opened before the write fails.
        let mut input = fs.input(&game, 0);
        let mut output = fs.output(&game, Some(0)).unwrap();
        probe.fail_a_write("game.bin").await;

        let results = [
            ("read", input.read(4).map(drop)),
            ("blocking read", input.blocking_read(4).await.map(drop)),
            ("write", output.write(b"x").map(drop)),
            ("read stream", fs.read(&game, 0).await.map(drop)),
            ("write stream", fs.write(&game, Some(0), "x").await),
            ("stat", fs.call(&Call::Stat(&game)).await),
            ("sync", fs.call(&Call::Sync(&game)).await),
            (
                "open",
                fs.call(&Call::Open(root, "new.bin", OpenFlags::CREATE))
                    .await,
            ),
        ];
        for (call, result) in results {
            check_error(result, &expected, call);
        }
    })
    .await;
}

/// Calls on `fd` whose results depend on what was written.
fn content_calls(fd: &Fd) -> [Call<'_>; 7] {
    [
        Call::Read(fd),
        Call::Stat(fd),
        Call::Sync(fd),
        Call::SyncData(fd),
        Call::SetSize(fd, 8),
        Call::SetTimes(fd),
        Call::MetadataHash(fd),
    ]
}

/// Calls on `path` whose results depend on what was written.
fn path_content_calls<'a>(dir: &'a Fd, path: &'a str) -> [Call<'a>; 3] {
    [
        Call::StatAt(dir, path),
        Call::SetTimesAt(dir, path),
        Call::MetadataHashAt(dir, path),
    ]
}

#[tokio::test]
async fn a_read_sees_the_writes_queued_before_it() {
    let mut h = Harness::new(&[]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let file = fs.open(&roots.download, "game.bin", OpenFlags::CREATE);
        let file = file.await.unwrap();
        let paused = probe.disk.pause();
        fs.write_now(&file, &["hello ", "world"]);

        let mut input = fs.input(&file, 0);
        let read = after_writes(paused, input.blocking_read(100)).await;
        assert_eq!(read.unwrap().unwrap(), b"hello world");
    })
    .await;
}

#[tokio::test]
async fn a_renamed_file_has_everything_written_before() {
    let mut h = Harness::new(&[]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let root = &roots.download;
        let file = fs.open(root, "state.tmp", OpenFlags::CREATE).await.unwrap();
        let paused = probe.disk.pause();
        fs.write_now(&file, &["resume ", "here"]);
        fs.close(file);

        let rename = Call::Rename(root, "state.tmp", root, "state");
        after_writes(paused, fs.call(&rename)).await.unwrap();
    })
    .await;

    assert_eq!(probe.read("state"), b"resume here");
    assert_eq!(probe.disk.landed(), 11);
}

#[tokio::test]
async fn calls_on_a_file_wait_for_its_writes_only() {
    let mut h = Harness::new(&["game.bin", "other.bin"]);
    let probe = h.probe();
    let linked = probe.download.join("linked.bin");
    fs::hard_link(probe.download.join("game.bin"), linked).unwrap();
    h.run(async |fs, roots| {
        let root = &roots.download;
        let open = async |name| fs.open(root, name, OpenFlags::empty()).await.unwrap();
        let game = open("game.bin").await;
        let game_again = open("game.bin").await;
        let other = open("other.bin").await;

        let truncates = ["game.bin", "linked.bin"].map(|name| {
            // Waits once the file is open.
            Call::Open(root, name, OpenFlags::TRUNCATE)
        });
        // The same file through another descriptor is the same file.
        for (held, waits) in [(&game, true), (&game_again, true), (&other, false)] {
            let held = fs.file(held);
            for call in content_calls(&game).iter().chain(&truncates) {
                probe.check_waits(fs, &held, waits, call).await.unwrap();
            }
        }
    })
    .await;
}

#[tokio::test]
async fn paths_that_could_have_writes_wait_for_all_of_them() {
    let entries = ["game.bin", "other.bin", "old.tmp", "trash.bin"];
    let mut h = Harness::new(&entries);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let root = &roots.download;
        let game = fs.open(root, "game.bin", OpenFlags::empty()).await.unwrap();
        let held = fs.file(&game);

        let calls = path_content_calls(root, "other.bin").into_iter().chain([
            Call::Rename(root, "old.tmp", root, "new"),
            Call::Unlink(root, "trash.bin"),
        ]);
        for call in calls {
            probe.check_waits(fs, &held, true, &call).await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn calls_that_do_not_depend_on_contents_never_wait() {
    let mut h = Harness::new(&["game.bin", "empty/", "levels/"]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let root = &roots.download;
        let game = fs.open(root, "game.bin", OpenFlags::empty()).await.unwrap();
        let held = fs.file(&game);
        let create = OpenFlags::CREATE | OpenFlags::TRUNCATE;

        let calls = [
            Call::GetType(&game),
            Call::GetFlags(&game),
            Call::Advise(&game),
            Call::ReadDirectory(root),
            Call::Open(root, "game.bin", OpenFlags::empty()),
            Call::Open(root, "game.bin", OpenFlags::CREATE),
            Call::Open(root, "new.bin", create),
            Call::Open(root, "levels/new.pak", create),
            Call::Open(root, "levels", OpenFlags::DIRECTORY),
            Call::StatAt(root, "levels"),
            Call::CreateDirectory(root, "new"),
            Call::RemoveDirectory(root, "empty"),
            Call::Link(root, "game.bin", root, "hard.bin"),
        ];
        for call in &calls {
            probe.check_waits(fs, &held, false, call).await.unwrap();
        }

        let missing = Call::StatAt(root, "missing");
        let missing = probe.check_waits(fs, &held, false, &missing).await;
        check_error(missing, &ErrorCode::NoEntry, "stat missing");
        let symlink = Call::Symlink(root, "game.bin", "link");
        match probe.check_waits(fs, &held, false, &symlink).await {
            Ok(()) => {}
            // Windows only allows symlinks in Developer Mode or as admin.
            Err(ErrorCode::NotPermitted) if cfg!(windows) => {}
            result => panic!("{symlink:?}: {result:?}"),
        }
    })
    .await;
}

type Opened = Result<(String, DescriptorFlags, bool, Option<Vec<u8>>), String>;

/// Opens `path` and returns what the plugin and the disk see afterwards,
/// including whether it can be renamed while open.
async fn opened<D>(
    store: &Accessor<State, D>,
    (root, dir): (&Fd, &Path),
    path: &str,
    (path_flags, oflags, flags): (PathFlags, OpenFlags, DescriptorFlags),
) -> Opened
where
    D: HostDescriptorWithStore<State>,
    for<'a> D::Data<'a>: HostDescriptor,
{
    let open = D::open_at(store, borrow(root), path_flags, path.into(), oflags, flags);
    let fd = open.await.map_err(|error| format!("{:?}", code(error)))?;
    let type_ = D::get_type(store, borrow(&fd)).await.unwrap();
    let flags = D::get_flags(store, borrow(&fd)).await.unwrap();
    let (path, renamed) = (dir.join(path), dir.join("renamed"));
    let renamable = fs::rename(&path, &renamed).is_ok();
    if renamable {
        fs::rename(&renamed, &path).unwrap();
    }
    close(store, fd);
    Ok((format!("{type_:?}"), flags, renamable, fs::read(path).ok()))
}

#[tokio::test]
async fn opening_works_like_stock_wasi() {
    let mut h = Harness::new(&["held.bin"]);
    let probe = h.probe();
    let held = probe.file("held.bin");
    let each = [
        OpenFlags::CREATE,
        OpenFlags::DIRECTORY,
        OpenFlags::EXCLUSIVE,
        OpenFlags::TRUNCATE,
    ];
    let oflags = (0..1 << each.len()).map(|set| {
        let picked = (0..each.len()).filter(|i| set & 1 << i != 0);
        picked.fold(OpenFlags::empty(), |oflags, i| oflags | each[i])
    });
    let flags = [
        DescriptorFlags::empty(),
        DescriptorFlags::READ,
        DescriptorFlags::WRITE,
        DescriptorFlags::READ | DescriptorFlags::WRITE,
    ];
    let cases: Vec<_> = [PathFlags::empty(), PathFlags::SYMLINK_FOLLOW]
        .into_iter()
        .flat_map(|path_flags| oflags.clone().map(move |oflags| (path_flags, oflags)))
        .flat_map(|(path_flags, oflags)| flags.map(|flags| (path_flags, oflags, flags)))
        .collect();

    h.run(async |fs, roots| {
        let stock = stock(fs.0);
        // With a write queued, truncating waits for the file's writes instead.
        for held_write in [false, true] {
            for dir in [&probe.download, &probe.data] {
                for side in ["stock", "view"] {
                    for i in 0..cases.len() {
                        let case = dir.join(format!("{side}/{held_write}/{i}"));
                        fs::create_dir_all(case.join("dir")).unwrap();
                        fs::write(case.join("file.bin"), "file").unwrap();
                    }
                }
            }
            let _paused = held_write.then(|| probe.hold_a_write(&held));
            for mount in [
                (&roots.download, probe.download.as_path()),
                (&roots.data, probe.data.as_path()),
            ] {
                for (i, &case) in cases.iter().enumerate() {
                    for name in ["missing", "file.bin", "dir"] {
                        let path = format!("{held_write}/{i}/{name}");
                        let (stock_path, view_path) =
                            (format!("stock/{path}"), format!("view/{path}"));
                        let expected = opened(&stock, mount, &stock_path, case).await;
                        let viewed = opened(fs.0, mount, &view_path, case);
                        let viewed = viewed.now_or_never().expect("opening waited");
                        assert_eq!(viewed, expected, "{view_path} with {case:?}");
                    }
                }
            }
        }
    })
    .await;
}

#[cfg(windows)]
#[tokio::test]
async fn opening_through_a_junction_works_like_stock_wasi() {
    let mut h = Harness::new(&[]);
    let probe = h.probe();
    let outside = TempDir::new().unwrap();
    let junction = |link: &Path, target: &Path| {
        let mklink = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .args([link, target])
            .output()
            .unwrap();
        assert!(mklink.status.success(), "{mklink:?}");
    };
    let mut targets = Vec::new();
    for (root, mount) in [(&probe.download, "download"), (&probe.data, "data")] {
        for side in ["stock", "view"] {
            let target = outside.path().join(format!("{mount}-{side}"));
            fs::create_dir(&target).unwrap();
            fs::write(target.join("outside.bin"), "outside").unwrap();
            let root = root.join(side);
            fs::create_dir_all(root.join("real")).unwrap();
            fs::write(root.join("real/inside.bin"), "inside").unwrap();
            junction(&root.join("out"), &target);
            junction(&root.join("in"), &root.join("real"));
            targets.push(target);
        }
    }

    h.run(async |fs, roots| {
        let stock = stock(fs.0);
        for mount in [
            (&roots.download, probe.download.as_path()),
            (&roots.data, probe.data.as_path()),
        ] {
            for path in [
                "out/outside.bin",
                "out/new.bin",
                "in/inside.bin",
                "in/new.bin",
            ] {
                for oflags in [OpenFlags::empty(), OpenFlags::CREATE, OpenFlags::TRUNCATE] {
                    let case = (PathFlags::SYMLINK_FOLLOW, oflags, DescriptorFlags::WRITE);
                    let stock_path = format!("stock/{path}");
                    let expected = opened(&stock, mount, &stock_path, case).await;
                    let viewed = opened(fs.0, mount, &format!("view/{path}"), case).await;
                    assert_eq!(viewed, expected, "{path} in {:?} with {oflags:?}", mount.1);
                }
            }
        }
    })
    .await;
    for target in targets {
        let entries: Vec<_> = fs::read_dir(&target).unwrap().map(Result::unwrap).collect();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(fs::read(target.join("outside.bin")).unwrap(), b"outside");
    }
}

#[tokio::test]
async fn a_truncating_open_that_fails_leaves_the_file_closed() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    let held = probe.file("game.bin");
    h.run(async |fs, roots| {
        let paused = probe.hold_a_write(&held);
        probe.queue_a_failing_write("game.bin");

        let truncate = fs.open(&roots.download, "game.bin", OpenFlags::TRUNCATE);
        assert!(after_writes(paused, truncate).await.is_err());
    })
    .await;

    drop(held);
    fs::remove_file(probe.download.join("game.bin")).unwrap();
}

async fn check_creating(name: &str, refused: bool) {
    let mut h = Harness::new(&["game.bin"]);
    h.run(async |fs, roots| {
        let root = &roots.download;
        let calls = [
            Call::Open(root, name, OpenFlags::CREATE),
            Call::CreateDirectory(root, name),
            Call::Link(root, "game.bin", root, name),
            Call::Symlink(root, "game.bin", name),
            Call::Rename(root, "game.bin", root, name),
        ];
        for call in calls {
            let result = fs.call(&call).await;
            let invalid = matches!(result, Err(ErrorCode::Invalid));
            assert_eq!(invalid, refused, "{call:?}: {result:?}");
        }
    })
    .await;
}

#[tokio::test]
async fn names_windows_cannot_use_are_refused() {
    let mut names = [
        "stream:of",
        "dir/stream:of",
        "trailing.",
        "trailing ",
        "con",
        "dir/nul.txt",
    ]
    .map(String::from)
    .to_vec();
    let forbidden = (1..32u8).map(char::from).chain(r#"<>:"|?*"#.chars());
    names.extend(forbidden.map(|c| format!("a{c}b")));

    for name in names {
        check_creating(&name, cfg!(windows)).await;
    }
}

#[tokio::test]
async fn other_names_are_created_everywhere() {
    let names = [
        "game.exe", "a b.txt", ".hidden", "console", "COM10", "nul_", "x.con", "名前", "./here",
        "dir/./x", "dir//x", "../up", "slash/",
    ];
    for name in names {
        check_creating(name, false).await;
    }
}

#[tokio::test]
async fn symlinks_never_point_outside() {
    let mut h = Harness::new(&[]);
    let probe = h.probe();
    fs::create_dir_all(probe.data.join("sub")).unwrap();
    fs::create_dir(probe.data.join("..x")).unwrap();
    fs::write(probe.data.join("sub").join("game.bin"), "game").unwrap();
    h.run(async |fs, roots| {
        let root = &roots.data;
        let refused = ["..", "../outside", "sub/../../outside", "a\\..\\b", "/etc"];
        for (i, target) in refused.into_iter().enumerate() {
            let link = format!("refused{i}");
            let result = fs.call(&Call::Symlink(root, target, &link)).await;
            assert!(
                matches!(result, Err(ErrorCode::NotPermitted)),
                "{target}: {result:?}"
            );
            assert!(!probe.data.join(&link).is_symlink(), "{target}");
        }
        for (i, target) in ["sub", "sub/game.bin", "./sub", "..x"]
            .into_iter()
            .enumerate()
        {
            match fs
                .call(&Call::Symlink(root, target, &format!("allowed{i}")))
                .await
            {
                Ok(()) => {}
                // Windows only allows symlinks in Developer Mode or as admin.
                Err(ErrorCode::NotPermitted) if cfg!(windows) => {}
                result => panic!("{target}: {result:?}"),
            }
        }
    })
    .await;
}

#[tokio::test]
async fn only_files_under_the_download_are_queued() {
    let mut h = Harness::new(&["sub/"]);
    let probe = h.probe();
    fs::create_dir(probe.data.join("sub")).unwrap();
    h.run(async |fs, roots| {
        let mounts = [
            ("download", &roots.download, true),
            ("data", &roots.data, false),
        ];
        for (mount, root, queued) in mounts {
            let sub = fs.open(root, "sub", OpenFlags::DIRECTORY).await.unwrap();
            for (dir, name) in [
                (root, format!("{mount}/a")),
                (&sub, format!("{mount}/sub/a")),
            ] {
                let file = fs.open(dir, "a", OpenFlags::CREATE).await.unwrap();
                probe.check_queued(fs, &file, queued, &name).await;
            }
        }
    })
    .await;
}

#[tokio::test]
async fn dropped_descriptors_are_no_longer_queued() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let game = fs.open(&roots.download, "game.bin", OpenFlags::empty());
        let game = game.await.unwrap();
        let rep = game.rep();
        fs.close(game);

        let file = fs.open(&roots.data, "state.json", OpenFlags::CREATE);
        let file = file.await.unwrap();
        assert_eq!(file.rep(), rep, "the table should reuse the dropped rep");
        probe.check_queued(fs, &file, false, "state.json").await;
    })
    .await;
}

#[tokio::test]
async fn data_writes_land_before_the_call_returns() {
    let mut h = Harness::new(&[]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let file = fs.open(&roots.data, "state.json", OpenFlags::CREATE);
        let file = file.await.unwrap();

        fs.write(&file, Some(0), "ab").await.unwrap();
        assert_eq!(probe.read_data("state.json"), b"ab");
        let mut output = fs.output(&file, Some(0)).unwrap();
        assert_eq!(output.write(b"cd").unwrap(), 2);
        assert_eq!(probe.read_data("state.json"), b"cd");
        assert_eq!(output.write(b"e").unwrap(), 1);
        assert_eq!(probe.read_data("state.json"), b"cde");
        let mut append = fs.output(&file, None).unwrap();
        assert_eq!(append.write(b"!").unwrap(), 1);
        assert_eq!(probe.read_data("state.json"), b"cde!");
    })
    .await;

    assert_eq!(
        probe.disk.landed(),
        0,
        "data writes aren't download progress"
    );
}

#[tokio::test]
async fn data_calls_never_wait_for_queued_writes() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    for name in ["state.json", "old.tmp", "trash.bin"] {
        fs::write(probe.data.join(name), name).unwrap();
    }
    h.run(async |fs, roots| {
        let data = &roots.data;
        let game = fs.open(&roots.download, "game.bin", OpenFlags::empty());
        let held = fs.file(&game.await.unwrap());
        let state = fs
            .open(data, "state.json", OpenFlags::empty())
            .await
            .unwrap();

        let calls = content_calls(&state)
            .into_iter()
            .chain([Call::Open(data, "state.json", OpenFlags::TRUNCATE)])
            .chain(path_content_calls(data, "state.json"))
            .chain([
                Call::Rename(data, "old.tmp", data, "new"),
                Call::Unlink(data, "trash.bin"),
            ]);
        for call in calls {
            probe.check_waits(fs, &held, false, &call).await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn files_cant_move_between_download_and_data() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    fs::write(probe.data.join("state.json"), "{}").unwrap();
    h.run(async |fs, roots| {
        let (download, data) = (&roots.download, &roots.data);
        for (from, name, to) in [(download, "game.bin", data), (data, "state.json", download)] {
            for call in [
                Call::Rename(from, name, to, name),
                Call::Link(from, name, to, name),
            ] {
                let result = fs.call(&call).await;
                check_error(result, &ErrorCode::CrossDevice, format!("{call:?}"));
            }
        }
    })
    .await;

    assert_eq!(probe.read("game.bin"), b"game.bin");
    assert_eq!(probe.read_data("state.json"), b"{}");
}

#[tokio::test]
async fn a_failed_download_write_leaves_data_working() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    probe.fail_a_write("game.bin").await;
    h.run(async |fs, roots| {
        let game = fs
            .open(&roots.download, "game.bin", OpenFlags::empty())
            .await;
        assert!(game.is_err(), "the download should have failed");

        let data = &roots.data;
        let file = fs
            .open(data, "state.json", OpenFlags::CREATE)
            .await
            .unwrap();
        fs.write(&file, Some(0), "ok").await.unwrap();
        assert_eq!(fs.read(&file, 0).await.unwrap(), b"ok");
        let mut input = fs.input(&file, 0);
        assert_eq!(input.read(2).unwrap().unwrap(), b"ok");
        drop(input);
        fs.close(file);
        let rename = Call::Rename(data, "state.json", data, "renamed.json");
        fs.call(&rename).await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn a_read_stream_waits_for_writes_queued_after_it_was_opened() {
    let mut h = Harness::new(&["game.bin"]);
    let probe = h.probe();
    h.run(async |fs, roots| {
        let root = &roots.download;
        let reading = fs.open(root, "game.bin", OpenFlags::empty()).await.unwrap();
        let writing = fs.open(root, "game.bin", OpenFlags::empty()).await.unwrap();
        let mut input = fs.input(&reading, 0);
        assert_eq!(input.blocking_read(4).await.unwrap().unwrap(), b"game");

        let paused = probe.disk.pause();
        fs.write_now(&writing, &["gameWXYZ"]);
        let read = input.read(4).unwrap().unwrap();
        assert_eq!(read, b"", "the write is still queued");
        assert!(!input.is_ready());
        drop(paused);
        input.ready().await;
        assert_eq!(input.read(4).unwrap().unwrap(), b"WXYZ");
    })
    .await;
}

#[tokio::test]
async fn a_read_stream_ends_only_at_the_end_of_the_file() {
    let mut h = Harness::new(&["game.bin"]);
    h.run(async |fs, roots| {
        let game = fs.open(&roots.download, "game.bin", OpenFlags::empty());
        let mut input = fs.input(&game.await.unwrap(), 0);

        let reads: [(usize, Option<&[u8]>); 5] = [
            (0, Some(b"")),
            (5, Some(b"game.")),
            (0, Some(b"")),
            (100, Some(b"bin")),
            (1, None),
        ];
        for (size, expected) in reads {
            let read = input.read(size).unwrap();
            assert_eq!(read.as_deref(), expected, "read({size})");
        }
    })
    .await;
}

#[tokio::test]
async fn a_read_stream_never_returns_overwritten_data() {
    let mut h = Harness::new(&["game.bin"]);
    h.run(async |fs, roots| {
        let game = fs.open(&roots.download, "game.bin", OpenFlags::empty());
        let game = game.await.unwrap();
        let mut input = fs.input(&game, 0);
        input.ready().await;
        assert_eq!(input.read(2).unwrap().unwrap(), b"ga");

        fs.write(&game, Some(2), "XXXXXX").await.unwrap();
        let read = input.blocking_read(6).await.unwrap().unwrap();
        assert_eq!(read, b"XXXXXX");
    })
    .await;
}

#[test]
fn a_dropped_read_stream_closes_its_file() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut h = Harness::new(&["game.bin"]);
        let probe = h.probe();
        // Holds up any read sent to a blocking thread, just like a slow disk.
        let (release, gate) = std::sync::mpsc::channel::<()>();
        let busy = tokio::task::spawn_blocking(move || gate.recv());

        let (still_open, removed) = h
            .run(async |fs, roots| {
                let game = fs.open(&roots.download, "game.bin", OpenFlags::empty());
                let game = game.await.unwrap();
                let opened = Arc::downgrade(&fs.file(&game));
                let mut input = fs.input(&game, 0);
                let _ = input.read(4);
                drop(input);
                fs.0.with(|mut store| {
                    let read = Fs::read_via_stream(reborrow(&mut store), borrow(&game), 0);
                    let (mut stream, mut result) = read.unwrap();
                    stream.close(&mut store).unwrap();
                    result.close(&mut store).unwrap();
                });
                fs.close(game);
                let still_open = opened.upgrade().is_some();
                (still_open, fs::remove_file(probe.download.join("game.bin")))
            })
            .await;

        release.send(()).unwrap();
        busy.await.unwrap().unwrap();
        assert!(!still_open, "a read outlived the stream");
        removed.unwrap();
    });
}

#[derive(Debug)]
enum Op {
    Open {
        name: usize,
        truncate: bool,
    },
    Close(usize),
    Stream {
        fd: usize,
        at: Option<u64>,
        chunks: Vec<usize>,
    },
    Write {
        fd: usize,
        at: u64,
        len: usize,
    },
    Read {
        fd: usize,
        at: u64,
        len: u64,
    },
    ReadStream {
        fd: usize,
        at: u64,
    },
    /// Continues reading from the file's kept stream, or opens one at `at`,
    /// wrapped to fall within the file.
    KeepReading {
        fd: usize,
        at: u64,
        len: usize,
    },
    Stat(usize),
    StatAt(usize),
    SetSize {
        fd: usize,
        len: u64,
    },
    Sync(usize),
    Rename {
        from: usize,
        to: usize,
    },
    Unlink(usize),
}

const NAMES: [&str; 4] = ["a.bin", "b.bin", "data/c.bin", "data/d.bin"];

fn random_op(rng: &mut StdRng, open: usize) -> Op {
    let size = |rng: &mut StdRng| match rng.random_range(0..10) {
        0 => rng.random_range(MAX_MERGED / 2..MAX_MERGED * 2),
        1..3 => rng.random_range(10_000..200_000),
        _ => rng.random_range(1..5_000),
    };
    let name = rng.random_range(0..NAMES.len());
    if open == 0 {
        return Op::Open {
            name,
            truncate: rng.random_bool(0.2),
        };
    }
    let fd = rng.random_range(0..open);
    let at = rng.random_range(0..300_000);
    match rng.random_range(0..16) {
        0 => Op::Open {
            name,
            truncate: rng.random_bool(0.2),
        },
        1 => Op::Close(fd),
        2..5 => {
            let chunks = (0..rng.random_range(1..4)).map(|_| size(rng)).collect();
            let at = rng.random_bool(0.8).then_some(at);
            Op::Stream { fd, at, chunks }
        }
        5 => Op::Write {
            fd,
            at,
            len: size(rng),
        },
        6 => Op::Read {
            fd,
            at,
            len: rng.random_range(0..300_000),
        },
        7 => Op::ReadStream { fd, at },
        8..10 => Op::KeepReading {
            fd,
            at,
            len: rng.random_range(1..20_000),
        },
        10 => Op::Stat(fd),
        11 => Op::StatAt(name),
        12 => Op::SetSize {
            fd,
            len: rng.random_range(0..400_000),
        },
        13 => Op::Sync(fd),
        14 => Op::Rename {
            from: name,
            to: rng.random_range(0..NAMES.len()),
        },
        _ => Op::Unlink(name),
    }
}

/// Bytes that differ by position and by write, so misplaced or stale data
/// shows.
fn pattern(seed: u64, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (seed as usize * 31 + i * 7 % 251) as u8)
        .collect()
}

fn write_model(model: &mut Vec<u8>, at: usize, data: &[u8]) {
    if model.len() < at + data.len() {
        model.resize(at + data.len(), 0);
    }
    model[at..at + data.len()].copy_from_slice(data);
}

struct Pacer {
    disk: Arc<Disk>,
    paused: Option<Paused>,
    /// Calls that couldn't finish at once.
    waits: usize,
}

impl Pacer {
    fn new(disk: Arc<Disk>, slow: bool) -> Self {
        let paused = slow.then(|| disk.pause());
        Self {
            disk,
            paused,
            waits: 0,
        }
    }

    async fn run<T>(&mut self, call: impl Future<Output = T>) -> T {
        let mut call = pin!(call);
        if let Some(output) = call.as_mut().now_or_never() {
            return output;
        }
        self.waits += 1;
        self.resume_for(call).await
    }

    /// Calls through the store take turns with the test, so they get a
    /// while to finish.
    async fn stream<T>(&mut self, call: impl Future<Output = T>) -> T {
        let mut call = pin!(call);
        if let Some(output) = settle(call.as_mut()).await {
            return output;
        }
        self.resume_for(call).await
    }

    async fn resume_for<T>(&mut self, call: impl Future<Output = T>) -> T {
        let slow = self.paused.take().is_some();
        let output = call.await;
        if slow {
            self.paused = Some(self.disk.pause());
        }
        output
    }
}

async fn read_stream(fs: Plugin<'_>, pacer: &mut Pacer, fd: &Fd, at: u64) -> Vec<u8> {
    let mut input = fs.input(fd, at);
    let mut data = Vec::new();
    while let Some(chunk) = pacer.run(input.blocking_read(64 * 1024)).await.unwrap() {
        data.extend_from_slice(&chunk);
    }
    data
}

/// Runs `count` random operations and returns how many had to wait.
async fn check_random_ops(seed: u64, count: usize, mode: Mode, slow: bool) -> usize {
    let mut h = Harness::with_mode(&["data/"], mode);
    let probe = h.probe();
    let mut model: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    let mut written = 0;
    let mut ops = Vec::new();

    let waits = h
        .run(async |fs, roots| {
            let root = &roots.download;
            let mut pacer = Pacer::new(probe.disk.clone(), slow);
            let mut rng = StdRng::seed_from_u64(seed);
            let mut fds: Vec<(&str, Fd)> = Vec::new();
            // A read stream per file kept across operations as wasi-libc does.
            let mut kept: BTreeMap<&str, (Input, usize)> = BTreeMap::new();

            for step in 0..count {
                let op = random_op(&mut rng, fds.len());
                let context = format!("seed {seed}, step {step}: {op:?}");
                match op {
                    Op::Open { name, truncate } => {
                        let name = NAMES[name];
                        let mut oflags = OpenFlags::CREATE;
                        if truncate {
                            oflags |= OpenFlags::TRUNCATE;
                        }
                        let fd = pacer
                            .run(fs.open(root, name, oflags))
                            .await
                            .expect(&context);
                        let contents = model.entry(name).or_default();
                        if truncate {
                            contents.clear();
                        }
                        fds.push((name, fd));
                    }
                    Op::Close(fd) => {
                        let (_, fd) = fds.remove(fd);
                        fs.close(fd);
                    }
                    Op::Stream { fd, at, chunks } => {
                        let (name, fd) = &fds[fd];
                        let mut output = fs.output(fd, at).expect(&context);
                        let mut position = at.map_or(model[name].len(), |at| at as usize);
                        for len in chunks {
                            let data = pattern(seed ^ step as u64, len);
                            let write = output.blocking_write(&data);
                            pacer.run(write).await.expect(&context);
                            write_model(model.get_mut(name).unwrap(), position, &data);
                            position += len;
                            written += len as u64;
                        }
                    }
                    Op::Write { fd, at, len } => {
                        let (name, fd) = &fds[fd];
                        let data = pattern(seed ^ step as u64, len);
                        let write = fs.write(fd, Some(at), data.clone());
                        pacer.stream(write).await.expect(&context);
                        write_model(model.get_mut(name).unwrap(), at as usize, &data);
                        written += len as u64;
                    }
                    Op::Read { fd, at, len } => {
                        let (name, fd) = &fds[fd];
                        let mut input = fs.input(fd, at);
                        let read = input.blocking_read(len as usize);
                        let data = pacer.run(read).await.expect(&context);
                        let data = data.unwrap_or_default();
                        let contents = &model[name];
                        let start = (at as usize).min(contents.len());
                        let end = (at + len).min(contents.len() as u64) as usize;
                        // Reads may be short, but never empty before the end.
                        let expected = &contents[start..end];
                        assert!(expected.starts_with(&data), "{context}: stale read");
                        assert!(
                            !data.is_empty() || expected.is_empty(),
                            "{context}: empty read"
                        );
                    }
                    Op::ReadStream { fd, at } => {
                        let (name, fd) = &fds[fd];
                        let data = read_stream(fs, &mut pacer, fd, at).await;
                        let contents = &model[name];
                        let start = (at as usize).min(contents.len());
                        assert!(data == contents[start..], "{context}: stale stream read");
                    }
                    Op::KeepReading { fd, at, len } => {
                        let (name, fd) = &fds[fd];
                        let (mut input, position) = match kept.remove(name) {
                            Some(kept) => kept,
                            None => {
                                let at = at % (model[name].len() as u64 + 1);
                                (fs.input(fd, at), at as usize)
                            }
                        };
                        let contents = &model[name];
                        match pacer.run(input.blocking_read(len)).await {
                            Ok(Some(data)) => {
                                let end = position + data.len();
                                assert!(end <= contents.len(), "{context}: read past the end");
                                let expected = &contents[position..end];
                                assert!(data == expected, "{context}: stale kept stream read");
                                kept.insert(name, (input, end));
                            }
                            Ok(None) => {
                                assert!(position >= contents.len(), "{context}: early end");
                            }
                            Err(error) => panic!("{context}: {error:?}"),
                        }
                    }
                    Op::Stat(fd) => {
                        let (name, fd) = &fds[fd];
                        let size = pacer.run(fs.size(fd)).await.expect(&context);
                        assert_eq!(size, model[name].len() as u64, "{context}");
                    }
                    Op::StatAt(name) => {
                        let name = NAMES[name];
                        let size = pacer.run(fs.size_at(root, name)).await;
                        match model.get(name) {
                            Some(contents) => {
                                let size = size.expect(&context);
                                assert_eq!(size, contents.len() as u64, "{context}");
                            }
                            None => assert!(size.is_err(), "{context}"),
                        }
                    }
                    Op::SetSize { fd, len } => {
                        let (name, fd) = &fds[fd];
                        let set_size = Call::SetSize(fd, len);
                        pacer.run(fs.call(&set_size)).await.expect(&context);
                        model.get_mut(name).unwrap().resize(len as usize, 0);
                    }
                    Op::Sync(fd) => {
                        let (_, fd) = &fds[fd];
                        pacer.run(fs.call(&Call::Sync(fd))).await.expect(&context);
                    }
                    Op::Rename { from, to } => {
                        let (from, to) = (NAMES[from], NAMES[to]);
                        if from == to || !model.contains_key(from) {
                            continue;
                        }
                        let renamed = fds.extract_if(.., |(name, _)| *name == from || *name == to);
                        for (_, fd) in renamed {
                            fs.close(fd);
                        }
                        kept.retain(|name, _| *name != from && *name != to);
                        let rename = Call::Rename(root, from, root, to);
                        pacer.run(fs.call(&rename)).await.expect(&context);
                        let contents = model.remove(from).unwrap();
                        model.insert(to, contents);
                    }
                    Op::Unlink(name) => {
                        let name = NAMES[name];
                        if !model.contains_key(name) {
                            continue;
                        }
                        for (_, fd) in fds.extract_if(.., |(open, _)| *open == name) {
                            fs.close(fd);
                        }
                        kept.remove(name);
                        let unlink = Call::Unlink(root, name);
                        pacer.run(fs.call(&unlink)).await.expect(&context);
                        model.remove(name);
                    }
                }
                ops.push(context);
            }

            let waits = pacer.waits;
            drop(pacer);
            drop(kept);
            probe.writer.drain().await.unwrap();
            for (_, fd) in fds {
                fs.close(fd);
            }
            waits
        })
        .await;

    for name in NAMES {
        let on_disk = fs::read(probe.download.join(name)).ok();
        assert!(
            on_disk.as_ref() == model.get(name),
            "seed {seed}: {name} differs after:\n{}",
            ops.join("\n")
        );
    }
    assert_eq!(probe.disk.landed(), written, "seed {seed}");
    waits
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn random_operations_match_a_model() {
    let mut inline_waits = 0;
    for seed in 0..40 {
        let mode = match seed % 4 {
            0 | 1 => Mode::Spawned,
            _ => Mode::Inline,
        };
        let slow = seed % 2 == 1;
        let waits = check_random_ops(seed, 60, mode, slow).await;
        if mode == Mode::Inline && slow {
            inline_waits += waits;
        }
    }
    assert!(inline_waits > 0, "no call ran while writes were queued");
}

type Fd = Resource<Descriptor>;
type FsAccessor = Accessor<State, Fs>;

/// Records the length of each write as it lands. While paused, it holds the
/// writer thread after each write, so the queue still sees that write in
/// flight just like a slow disk.
#[derive(Default)]
struct Disk {
    state: Mutex<DiskState>,
    changed: Condvar,
}

#[derive(Default)]
struct DiskState {
    writes: Vec<u64>,
    paused: bool,
    holding: bool,
}

impl Disk {
    fn pause(self: &Arc<Self>) -> Paused {
        self.lock().paused = true;
        Paused(self.clone())
    }

    fn wait_until_holding(&self) {
        let state = self.lock();
        let _state = self.changed.wait_while(state, |state| !state.holding);
    }

    /// The length of each write that reached the disk. Merged writes count
    /// as a single write.
    fn writes(&self) -> Vec<u64> {
        self.lock().writes.clone()
    }

    fn landed(&self) -> u64 {
        self.lock().writes.iter().sum()
    }

    fn lock(&self) -> MutexGuard<'_, DiskState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl DownloadReporter for Disk {
    fn report(&self, _phase: DownloadPhase) {}

    fn received(&self, _bytes: u64) {}

    fn written(&self, bytes: u64) {
        let mut state = self.lock();
        state.writes.push(bytes);
        state.holding = true;
        self.changed.notify_all();
        let mut state = self
            .changed
            .wait_while(state, |state| state.paused)
            .unwrap_or_else(PoisonError::into_inner);
        state.holding = false;
    }
}

struct Paused(Arc<Disk>);

impl Drop for Paused {
    fn drop(&mut self) {
        self.0.lock().paused = false;
        self.0.changed.notify_all();
    }
}

pub fn open_rw(path: &Path) -> Arc<File> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .unwrap();
    Arc::new(file)
}

struct State {
    ctx: WasiCtx,
    table: ResourceTable,
    writer: Writer,
    queued: QueuedFds,
    inline: bool,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

impl FsState for State {
    fn fs(&mut self) -> FsView<'_> {
        let fs = WasiFilesystemCtxView {
            ctx: self.ctx.filesystem(),
            table: &mut self.table,
        };
        FsView::new(fs, &self.writer, &mut self.queued, self.inline)
    }
}

/// Whether stock calls run on the calling task or on blocking threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Inline,
    Spawned,
}

struct Harness {
    download: TempDir,
    data: TempDir,
    store: Store<State>,
    writer: Writer,
    disk: Arc<Disk>,
}

struct Roots {
    download: Fd,
    data: Fd,
}

impl Harness {
    /// Creates `entries` in the download dir. Dirs if they end in `/` and
    /// files otherwise.
    fn new(entries: &[&str]) -> Self {
        Self::with_mode(entries, Mode::Inline)
    }

    fn with_mode(entries: &[&str], mode: Mode) -> Self {
        let download = TempDir::new().unwrap();
        for entry in entries {
            match entry.strip_suffix('/') {
                Some(dir) => fs::create_dir_all(download.path().join(dir)).unwrap(),
                None => fs::write(download.path().join(entry), entry).unwrap(),
            }
        }
        let data = TempDir::new().unwrap();
        let inline = mode == Mode::Inline;
        let mut ctx = WasiCtx::builder();
        ctx.allow_blocking_current_thread(inline)
            .preopened_dir(download.path(), DOWNLOAD_MOUNT, FsPerms::ReadWrite)
            .unwrap()
            .preopened_dir(data.path(), DATA_MOUNT, FsPerms::ReadWrite)
            .unwrap();
        let disk = Arc::new(Disk::default());
        let writer = Writer::new(Some(disk.clone()));
        let state = State {
            ctx: ctx.build(),
            table: ResourceTable::new(),
            writer: writer.clone(),
            queued: QueuedFds::default(),
            inline,
        };
        let mut config = Config::new();
        config.wasm_component_model_async(true);
        let engine = Engine::new(&config).unwrap();
        Self {
            download,
            data,
            store: Store::new(&engine, state),
            writer,
            disk,
        }
    }

    fn probe(&self) -> Probe {
        Probe {
            writer: self.writer.clone(),
            disk: self.disk.clone(),
            download: self.download.path().to_owned(),
            data: self.data.path().to_owned(),
        }
    }

    async fn run<R>(&mut self, test: impl AsyncFnOnce(Plugin<'_>, Roots) -> R) -> R {
        let run = self.store.run_concurrent(async |accessor| {
            let fs = accessor.with_getter::<Fs>(State::fs);
            let get_directories = |mut store: Access<'_, State, Fs>| {
                preopens::Host::get_directories(&mut store.get())
            };
            let mut dirs = fs.with(get_directories).unwrap();
            let mut root = |mount| {
                let i = dirs.iter().position(|(_, name)| name == mount).unwrap();
                dirs.remove(i).0
            };
            let roots = Roots {
                download: root(DOWNLOAD_MOUNT),
                data: root(DATA_MOUNT),
            };
            test(Plugin(&fs), roots).await
        });
        run.await.unwrap()
    }
}

/// Direct access to the writer, the disk and the mounted directories, without
/// going through the plugin's calls.
struct Probe {
    writer: Writer,
    disk: Arc<Disk>,
    download: PathBuf,
    data: PathBuf,
}

impl Probe {
    fn read(&self, name: &str) -> Vec<u8> {
        fs::read(self.download.join(name)).unwrap()
    }

    fn read_data(&self, name: &str) -> Vec<u8> {
        fs::read(self.data.join(name)).unwrap()
    }

    /// Opens a download file to queue writes to directly.
    fn file(&self, name: &str) -> Arc<File> {
        open_rw(&self.download.join(name))
    }

    fn push(&self, file: &Arc<File>, at: At, data: impl AsRef<[u8]>) -> io::Result<()> {
        let id = self.writer.id_of(file)?;
        let data = Bytes::copy_from_slice(data.as_ref());
        self.writer.push(file.clone(), id, at, data)
    }

    fn queued_bytes(&self) -> usize {
        self.writer.queued_bytes()
    }

    /// Pauses the disk while it writes to `held`.
    fn hold_a_write(&self, held: &Arc<File>) -> Paused {
        let paused = self.disk.pause();
        self.push(held, At::End, "!").unwrap();
        self.disk.wait_until_holding();
        paused
    }

    fn queue_a_failing_write(&self, name: &str) {
        let read_only = Arc::new(File::open(self.download.join(name)).unwrap());
        self.push(&read_only, At::Offset(0), "x").unwrap();
    }

    async fn fail_a_write(&self, name: &str) {
        self.queue_a_failing_write(name);
        assert!(self.writer.drain().await.is_err());
    }

    /// Writes through `fd` and checks whether the write was queued, which
    /// counts it as download progress.
    async fn check_queued(&self, fs: Plugin<'_>, fd: &Fd, queued: bool, name: &str) {
        let landed = self.disk.landed();
        fs.write(fd, Some(0), "a").await.unwrap();
        self.writer.drain().await.unwrap();
        let was_queued = self.disk.landed() > landed;
        assert_eq!(was_queued, queued, "{name} was queued: {was_queued}");
    }

    /// Makes `call` while a write to `held` is in flight, and checks
    /// whether it waited for that write.
    async fn check_waits(
        &self,
        fs: Plugin<'_>,
        held: &Arc<File>,
        waits: bool,
        call: &Call<'_>,
    ) -> Result<(), ErrorCode> {
        let paused = self.hold_a_write(held);
        let mut result = pin!(fs.call(call));
        let early = result.as_mut().now_or_never();
        drop(paused);
        self.writer.drain().await.unwrap();

        let waited = early.is_none();
        assert_eq!(waited, waits, "{call:?} waited: {waited}");
        match early {
            Some(result) => result,
            None => result.await,
        }
    }
}

/// An fs call made only to see whether it fails.
#[derive(Debug)]
enum Call<'a> {
    Read(&'a Fd),
    Stat(&'a Fd),
    Sync(&'a Fd),
    SyncData(&'a Fd),
    SetSize(&'a Fd, u64),
    SetTimes(&'a Fd),
    MetadataHash(&'a Fd),
    GetType(&'a Fd),
    GetFlags(&'a Fd),
    Advise(&'a Fd),
    ReadDirectory(&'a Fd),
    Open(&'a Fd, &'a str, OpenFlags),
    StatAt(&'a Fd, &'a str),
    SetTimesAt(&'a Fd, &'a str),
    MetadataHashAt(&'a Fd, &'a str),
    CreateDirectory(&'a Fd, &'a str),
    RemoveDirectory(&'a Fd, &'a str),
    Link(&'a Fd, &'a str, &'a Fd, &'a str),
    Symlink(&'a Fd, &'a str, &'a str),
    Rename(&'a Fd, &'a str, &'a Fd, &'a str),
    Unlink(&'a Fd, &'a str),
}

#[derive(Clone, Copy)]
struct Plugin<'a>(&'a FsAccessor);

impl Plugin<'_> {
    /// Opens for reading. Also opens for writing unless it's a directory.
    async fn open(self, dir: &Fd, path: &str, oflags: OpenFlags) -> Result<Fd, ErrorCode> {
        let flags = match oflags.contains(OpenFlags::DIRECTORY) {
            true => DescriptorFlags::READ,
            false => DescriptorFlags::READ | DescriptorFlags::WRITE,
        };
        self.open_with(dir, path, oflags, flags).await
    }

    async fn open_with(
        self,
        dir: &Fd,
        path: &str,
        oflags: OpenFlags,
        flags: DescriptorFlags,
    ) -> Result<Fd, ErrorCode> {
        let path_flags = PathFlags::empty();
        let open = Fs::open_at(self.0, borrow(dir), path_flags, path.into(), oflags, flags);
        open.await.map_err(code)
    }

    fn close(self, fd: Fd) {
        close(self.0, fd);
    }

    /// Writes `data` at `at` or appends it through a stream.
    async fn write(
        self,
        fd: &Fd,
        at: Option<u64>,
        data: impl Into<Bytes>,
    ) -> Result<(), ErrorCode> {
        let data = data.into();
        let result = self.0.with(|mut store| -> wasmtime::Result<_> {
            let stream = StreamReader::new(&mut store, data)?;
            let future = match at {
                Some(at) => Fs::write_via_stream(reborrow(&mut store), borrow(fd), stream, at)?,
                None => Fs::append_via_stream(reborrow(&mut store), borrow(fd), stream)?,
            };
            receive(&mut store, future)
        });
        result.unwrap().await.unwrap()
    }

    /// Writes `chunks` from the start through one stream without waiting.
    fn write_now(self, fd: &Fd, chunks: &[&str]) {
        let mut output = self.output(fd, Some(0)).unwrap();
        for chunk in chunks {
            assert_eq!(output.write(chunk.as_bytes()).unwrap(), chunk.len());
        }
    }

    /// Reads from `at` to the end through a stream.
    async fn read(self, fd: &Fd, at: u64) -> Result<Vec<u8>, ErrorCode> {
        let (data, result) = self
            .0
            .with(|mut store| -> wasmtime::Result<_> {
                let (stream, future) = Fs::read_via_stream(reborrow(&mut store), borrow(fd), at)?;
                let (done, data) = oneshot::channel();
                let collect = Collect {
                    data: Vec::new(),
                    done: Some(done),
                };
                stream.pipe(&mut store, collect)?;
                Ok((data, receive(&mut store, future)?))
            })
            .unwrap();
        let data = data.await.unwrap();
        result.await.unwrap().map(|()| data)
    }

    /// A write stream from `at` or appending driven directly.
    fn output(self, fd: &Fd, at: Option<u64>) -> Result<Output, ErrorCode> {
        let at = at.map_or(At::End, At::Offset);
        let consumer = self.0.with(|mut store| store.get().file_consumer(fd, at));
        consumer.map(Output).map_err(code)
    }

    /// A read stream from `at` driven directly.
    fn input(self, fd: &Fd, at: u64) -> Input {
        let producer = self.0.with(|mut store| store.get().file_producer(fd, at));
        Input(producer.unwrap().expect("not a file"))
    }

    async fn size(self, fd: &Fd) -> Result<u64, ErrorCode> {
        let stat = Fs::stat(self.0, borrow(fd)).await;
        stat.map(|stat| stat.size).map_err(code)
    }

    async fn size_at(self, dir: &Fd, path: &str) -> Result<u64, ErrorCode> {
        let stat = Fs::stat_at(self.0, borrow(dir), PathFlags::empty(), path.into()).await;
        stat.map(|stat| stat.size).map_err(code)
    }

    fn file(self, fd: &Fd) -> Arc<File> {
        self.0
            .with(|mut store| match store.get().fs.table.get(fd).unwrap() {
                Descriptor::File(file) => file.file.clone(),
                Descriptor::Dir(_) => panic!("not a file"),
            })
    }

    async fn call(self, call: &Call<'_>) -> Result<(), ErrorCode> {
        let fs = self.0;
        let no_flags = PathFlags::empty();
        let unchanged = || NewTimestamp::NoChange;
        let result = match *call {
            Call::Read(fd) => return self.input(fd, 0).blocking_read(10).await.map(drop),
            Call::Open(dir, path, oflags) => {
                return self.open(dir, path, oflags).await.map(drop);
            }
            Call::ReadDirectory(dir) => {
                let list = fs.with(|mut store| -> wasmtime::Result<()> {
                    let (mut entries, mut result) =
                        Fs::read_directory(reborrow(&mut store), borrow(dir))?;
                    entries.close(&mut store)?;
                    result.close(&mut store)
                });
                list.unwrap();
                return Ok(());
            }
            Call::Stat(fd) => Fs::stat(fs, borrow(fd)).await.map(drop),
            Call::Sync(fd) => Fs::sync(fs, borrow(fd)).await,
            Call::SyncData(fd) => Fs::sync_data(fs, borrow(fd)).await,
            Call::SetSize(fd, size) => Fs::set_size(fs, borrow(fd), size).await,
            Call::SetTimes(fd) => Fs::set_times(fs, borrow(fd), unchanged(), unchanged()).await,
            Call::MetadataHash(fd) => Fs::metadata_hash(fs, borrow(fd)).await.map(drop),
            Call::GetType(fd) => Fs::get_type(fs, borrow(fd)).await.map(drop),
            Call::GetFlags(fd) => Fs::get_flags(fs, borrow(fd)).await.map(drop),
            Call::Advise(fd) => Fs::advise(fs, borrow(fd), 0, 0, Advice::Normal).await,
            Call::StatAt(dir, path) => Fs::stat_at(fs, borrow(dir), no_flags, path.into())
                .await
                .map(drop),
            Call::SetTimesAt(dir, path) => {
                let path = path.into();
                Fs::set_times_at(fs, borrow(dir), no_flags, path, unchanged(), unchanged()).await
            }
            Call::MetadataHashAt(dir, path) => {
                Fs::metadata_hash_at(fs, borrow(dir), no_flags, path.into())
                    .await
                    .map(drop)
            }
            Call::CreateDirectory(dir, path) => {
                Fs::create_directory_at(fs, borrow(dir), path.into()).await
            }
            Call::RemoveDirectory(dir, path) => {
                Fs::remove_directory_at(fs, borrow(dir), path.into()).await
            }
            Call::Link(dir, from, to_dir, to) => {
                let (from, to) = (from.into(), to.into());
                Fs::link_at(fs, borrow(dir), no_flags, from, borrow(to_dir), to).await
            }
            Call::Symlink(dir, target, path) => {
                Fs::symlink_at(fs, borrow(dir), target.into(), path.into()).await
            }
            Call::Rename(dir, from, to_dir, to) => {
                Fs::rename_at(fs, borrow(dir), from.into(), borrow(to_dir), to.into()).await
            }
            Call::Unlink(dir, path) => Fs::unlink_file_at(fs, borrow(dir), path.into()).await,
        };
        result.map_err(code)
    }
}

/// A write stream as a plugin drives it, one write at a time.
struct Output(FileConsumer);

impl Output {
    /// Writes what the queue has room for now and returns how much.
    fn write(&mut self, data: &[u8]) -> Result<usize, ErrorCode> {
        match now(|cx| self.0.poll_write(cx, data)) {
            Poll::Ready(written) => Ok(written?),
            Poll::Pending => Ok(0),
        }
    }

    /// Writes as a cancelled write would, returning `None` if it took nothing.
    fn cancel(&mut self, data: &[u8]) -> Option<usize> {
        match now(|cx| self.0.poll_write(cx, data)) {
            Poll::Ready(written) => Some(written.unwrap()),
            Poll::Pending => {
                self.0.waiting = None;
                None
            }
        }
    }

    fn is_ready(&mut self) -> bool {
        now(|cx| self.0.poll_write(cx, &[])).is_ready()
    }

    async fn blocking_write(&mut self, mut data: &[u8]) -> Result<(), ErrorCode> {
        while !data.is_empty() {
            let written = poll_fn(|cx| self.0.poll_write(cx, data)).await?;
            data = &data[written..];
        }
        Ok(())
    }
}

/// A read stream as a plugin drives it, one read at a time. `None` is the end.
struct Input(FileProducer);

impl Input {
    /// Reads what's there now. Nothing while writes are queued.
    fn read(&mut self, size: usize) -> Result<Option<Vec<u8>>, ErrorCode> {
        let mut buf = vec![0; size];
        match now(|cx| self.0.poll_read(cx, &mut buf)) {
            Poll::Ready(read) => Ok(Self::filled(buf, read?)),
            Poll::Pending => Ok(Some(Vec::new())),
        }
    }

    async fn blocking_read(&mut self, size: usize) -> Result<Option<Vec<u8>>, ErrorCode> {
        let mut buf = vec![0; size];
        let read = poll_fn(|cx| self.0.poll_read(cx, &mut buf)).await?;
        Ok(Self::filled(buf, read))
    }

    fn is_ready(&mut self) -> bool {
        now(|cx| self.0.poll_read(cx, &mut [])).is_ready()
    }

    async fn ready(&mut self) {
        let _ = poll_fn(|cx| self.0.poll_read(cx, &mut [])).await;
    }

    fn filled(mut buf: Vec<u8>, len: usize) -> Option<Vec<u8>> {
        if len == 0 && !buf.is_empty() {
            return None;
        }
        buf.truncate(len);
        Some(buf)
    }
}

struct Receive<T>(Option<oneshot::Sender<T>>);

impl<T: Lift + Send + Sync + 'static, D> FutureConsumer<D> for Receive<T> {
    type Item = T;

    fn poll_consume(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        store: StoreContextMut<D>,
        mut source: Source<'_, T>,
        _finish: bool,
    ) -> Poll<wasmtime::Result<()>> {
        let mut value = None;
        source.read(store, &mut value)?;
        if let (Some(value), Some(sender)) = (value, self.0.take()) {
            let _ = sender.send(value);
        }
        Poll::Ready(Ok(()))
    }
}

/// Reads a whole stream and hands it to the test once it ends.
struct Collect {
    data: Vec<u8>,
    done: Option<oneshot::Sender<Vec<u8>>>,
}

impl<D> StreamConsumer<D> for Collect {
    type Item = u8;

    fn poll_consume(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        store: StoreContextMut<D>,
        source: Source<'_, u8>,
        _finish: bool,
    ) -> Poll<wasmtime::Result<StreamResult>> {
        let mut source = source.as_direct(store);
        let chunk = source.remaining();
        let len = chunk.len();
        self.data.extend_from_slice(chunk);
        source.mark_read(len);
        Poll::Ready(Ok(StreamResult::Completed))
    }
}

impl Drop for Collect {
    fn drop(&mut self) {
        if let Some(done) = self.done.take() {
            let _ = done.send(mem::take(&mut self.data));
        }
    }
}

fn reborrow<'a>(store: &'a mut Access<'_, State, Fs>) -> Access<'a, State, Fs> {
    Access::new(store.as_context_mut(), |state| state.fs())
}

fn receive<T: Lift + Send + Sync + 'static>(
    store: &mut Access<'_, State, Fs>,
    future: FutureReader<T>,
) -> wasmtime::Result<oneshot::Receiver<T>> {
    let (sender, receiver) = oneshot::channel();
    future.pipe(store, Receive(Some(sender)))?;
    Ok(receiver)
}

fn close<D>(store: &Accessor<State, D>, fd: Fd)
where
    D: HasData,
    for<'a> D::Data<'a>: HostDescriptor,
{
    store
        .with(|mut store| HostDescriptor::drop(&mut store.get(), fd))
        .unwrap();
}

/// The error code of a failed call, which shouldn't trap.
fn code(error: FsError) -> ErrorCode {
    error.downcast().unwrap()
}

/// `ErrorCode` has no `PartialEq`.
#[track_caller]
fn check_error<T: std::fmt::Debug>(
    result: Result<T, ErrorCode>,
    expected: &ErrorCode,
    context: impl std::fmt::Display,
) {
    let actual = format!("{:?}", result.err());
    assert_eq!(actual, format!("{:?}", Some(expected)), "{context}");
}

/// Yields until `call` is done, or for as long as a slow disk could take.
async fn settle<T>(call: impl Future<Output = T>) -> Option<T> {
    let mut call = pin!(call);
    for _ in 0..100 {
        if let Some(output) = call.as_mut().now_or_never() {
            return Some(output);
        }
        tokio::task::yield_now().await;
    }
    None
}

/// Checks that `call` waits for the writes queued while `paused`, and
/// returns its result once they land.
async fn after_writes<T>(paused: Paused, call: impl Future<Output = T>) -> T {
    let mut call = pin!(call);
    let early = call.as_mut().now_or_never();
    assert!(
        early.is_none(),
        "the call didn't wait for the queued writes"
    );
    drop(paused);
    call.await
}

/// Polls once, as a plugin's non-blocking stream call would.
fn now<T>(poll: impl FnOnce(&mut Context<'_>) -> Poll<T>) -> Poll<T> {
    poll(&mut Context::from_waker(std::task::Waker::noop()))
}
