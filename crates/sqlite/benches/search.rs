//! Library search benchmarks over a large, cached fixture database.
//!
//! ```text
//! cargo bench -p sqlite                              # ~2,000,000 games
//! THRUSTR_BENCH_GAMES=5000000 cargo bench -p sqlite  # bigger dataset
//! ```
//!
//! The first run generates the fixture under `target/tmp/` (a few minutes for
//! millions of rows) and every later run reuses it. Delete the `.done` marker
//! next to the `.sqlite` file to force a rebuild.

use std::{fs, hint::black_box, path::PathBuf, time::Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use domain::game::{GameRepository, GameSource, NewGame};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use sqlite::SqliteStorage;

const DEFAULT_GAMES: usize = 2_000_000;
const INSERT_BATCH: usize = 5_000;

/// A controlled vocabulary of ~3k pronounceable pseudo-words. Titles are built
/// from these, so the distinct-term count (what spellfix scales with) stays
/// realistic and independent of the row count.
fn vocabulary() -> Vec<String> {
    const ONSET: &[&str] = &[
        "b", "c", "d", "f", "g", "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "w", "z", "br",
        "cr", "dr", "gl", "gr", "pl", "pr", "sh", "sk", "sl", "st", "th", "tr", "vr", "wr",
    ];
    const NUCLEUS: &[&str] = &["a", "e", "i", "o", "u", "ai", "au", "ea", "ee", "io", "ou"];

    let mut rng = StdRng::seed_from_u64(0x0000_0001);
    let mut words: Vec<String> = (0..3_600)
        .map(|_| {
            let syllables = rng.random_range(2..=3);
            let mut word = String::new();
            for _ in 0..syllables {
                word.push_str(ONSET[rng.random_range(0..ONSET.len())]);
                word.push_str(NUCLEUS[rng.random_range(0..NUCLEUS.len())]);
            }
            word
        })
        .collect();
    words.sort();
    words.dedup();
    words
}

fn title(rng: &mut StdRng, vocab: &[String]) -> String {
    let count = rng.random_range(1..=4);
    let mut parts: Vec<String> = (0..count)
        .map(|_| vocab[rng.random_range(0..vocab.len())].clone())
        .collect();
    if rng.random_bool(0.1) {
        parts.push(rng.random_range(2..100).to_string());
    }
    parts.join(" ")
}

fn fixture_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("search-bench");
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn game_count() -> usize {
    std::env::var("THRUSTR_BENCH_GAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_GAMES)
}

fn seed_games(
    storage: &SqliteStorage,
    from: usize,
    count: usize,
    rng: &mut StdRng,
    vocab: &[String],
) {
    let mut made = 0;
    while made < count {
        let take = INSERT_BATCH.min(count - made);
        let batch: Vec<NewGame> = (0..take)
            .map(|k| NewGame {
                name: title(rng, vocab),
                source: GameSource {
                    id: "bench".to_string(),
                    lookup_id: (from + made + k).to_string(),
                    external_ids: Default::default(),
                },
                cover_url: None,
                summary: None,
                description: None,
            })
            .collect();
        storage.insert_many(&batch).unwrap();
        made += take;
    }
}

/// Opens (building on first use) the shared read-only fixture.
fn fixture(vocab: &[String]) -> SqliteStorage {
    let count = game_count();
    let dir = fixture_dir();
    let db = dir.join(format!("games-{count}.sqlite"));
    let marker = dir.join(format!("games-{count}.done"));

    if !marker.exists() {
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(dir.join(format!("games-{count}.sqlite{suffix}")));
        }
        let storage = SqliteStorage::new(&db).unwrap();
        let mut rng = StdRng::seed_from_u64(0xC0FF_EE01);
        let start = Instant::now();
        seed_games(&storage, 0, count, &mut rng, vocab);
        eprintln!(
            "built {count}-row search fixture in {:.1}s",
            start.elapsed().as_secs_f64()
        );
        fs::write(&marker, "").unwrap();
    }

    SqliteStorage::new(&db).unwrap()
}

fn one_edit(rng: &mut StdRng, word: &str) -> String {
    let letters = b"abcdefghijklmnopqrstuvwxyz";
    let mut chars: Vec<char> = word.chars().collect();
    if chars.len() < 3 {
        return word.to_string();
    }
    let at = rng.random_range(0..chars.len());
    match rng.random_range(0..4) {
        0 => {
            chars.remove(at);
        }
        1 if at + 1 < chars.len() => chars.swap(at, at + 1),
        2 => chars[at] = letters[rng.random_range(0..26)] as char,
        _ => chars.insert(at, letters[rng.random_range(0..26)] as char),
    }
    chars.into_iter().collect()
}

fn gibberish(rng: &mut StdRng) -> String {
    let letters = b"abcdefghijklmnopqrstuvwxyz";
    (0..rng.random_range(5..9))
        .map(|_| letters[rng.random_range(0..26)] as char)
        .collect()
}

struct Queries {
    exact: Vec<String>,
    prefix: Vec<String>,
    typo: Vec<String>,
    phrase: Vec<String>,
    no_match: Vec<String>,
}

fn queries(vocab: &[String]) -> Queries {
    let mut rng = StdRng::seed_from_u64(0x0009_9999);
    let word = |rng: &mut StdRng| vocab[rng.random_range(0..vocab.len())].clone();

    let exact: Vec<String> = (0..50).map(|_| word(&mut rng)).collect();
    let prefix = exact
        .iter()
        .map(|w| w.chars().take(4).collect::<String>())
        .collect();
    let typo = exact.iter().map(|w| one_edit(&mut rng, w)).collect();
    let phrase = (0..50)
        .map(|_| format!("{} {}", word(&mut rng), word(&mut rng)))
        .collect();
    let no_match = (0..50).map(|_| gibberish(&mut rng)).collect();

    Queries {
        exact,
        prefix,
        typo,
        phrase,
        no_match,
    }
}

fn run(storage: &SqliteStorage, batch: &[String]) {
    for query in batch {
        black_box(storage.list_index(Some(query)).unwrap());
    }
}

fn search_benches(c: &mut Criterion) {
    let vocab = vocabulary();
    let storage = fixture(&vocab);
    let q = queries(&vocab);

    let mut group = c.benchmark_group("search");
    group.sample_size(20);
    group.bench_function("exact", |b| b.iter(|| run(&storage, &q.exact)));
    group.bench_function("prefix", |b| b.iter(|| run(&storage, &q.prefix)));
    group.bench_function("typo", |b| b.iter(|| run(&storage, &q.typo)));
    group.bench_function("phrase", |b| b.iter(|| run(&storage, &q.phrase)));
    group.bench_function("no_match", |b| b.iter(|| run(&storage, &q.no_match)));
    group.finish();
}

fn insert_benches(c: &mut Criterion) {
    // A fresh DB pre-seeded enough that the word vocabulary is mostly (not fully)
    // saturated - the realistic steady state for a storefront sync.
    let vocab = vocabulary();
    let dir = fixture_dir();
    let db = dir.join("insert.sqlite");
    for suffix in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(dir.join(format!("insert.sqlite{suffix}")));
    }
    let storage = SqliteStorage::new(&db).unwrap();
    let mut rng = StdRng::seed_from_u64(0x1111_2222);
    seed_games(&storage, 0, 40_000, &mut rng, &vocab);

    let mut next = 40_000usize;
    let mut group = c.benchmark_group("index");
    group.sample_size(10);
    group.bench_function("insert_1k", |b| {
        b.iter(|| {
            seed_games(&storage, next, 1_000, &mut rng, &vocab);
            next += 1_000;
        })
    });
    group.finish();
}

criterion_group!(benches, search_benches, insert_benches);
criterion_main!(benches);
