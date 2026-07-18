//! Dev-only tool to fill the local database with fake games, for testing
//! list virtualization/scrolling at scale. Writes to the real dev DB path.
//! Usage: `cargo run -p sqlite --example seed_games -- [count]` (default 20000)

use domain::game::{GameRepository, GameSource, NewGame};
use sqlite::SqliteStorage;
use std::collections::HashMap;

const DEFAULT_COUNT: usize = 20_000;
const INSERT_BATCH: usize = 1000;

const ADJECTIVES: &[&str] = &[
    "Shadow", "Crimson", "Silent", "Broken", "Eternal", "Rogue", "Ancient", "Neon", "Frozen",
    "Savage", "Hollow", "Radiant", "Iron", "Lost", "Golden", "Feral",
];

const NOUNS: &[&str] = &[
    "Legacy",
    "Protocol",
    "Empire",
    "Horizon",
    "Vendetta",
    "Odyssey",
    "Requiem",
    "Uprising",
    "Sanctuary",
    "Dominion",
    "Frontier",
    "Paradox",
    "Covenant",
    "Wasteland",
    "Nexus",
    "Reckoning",
];

fn game_name(index: usize) -> String {
    let adj = ADJECTIVES[index % ADJECTIVES.len()];
    let noun = NOUNS[(index / ADJECTIVES.len()) % NOUNS.len()];
    format!("{adj} {noun} {index}")
}

fn main() {
    let count = std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_COUNT);

    let db_path = config::paths::db_path();
    println!("seeding {count} games into {}", db_path.display());

    let storage = SqliteStorage::new(&db_path)
        .unwrap_or_else(|e| panic!("failed to open database at {}: {e}", db_path.display()));

    let mut inserted = 0;
    for batch_start in (0..count).step_by(INSERT_BATCH) {
        let batch_end = (batch_start + INSERT_BATCH).min(count);
        let games: Vec<NewGame> = (batch_start..batch_end)
            .map(|i| NewGame {
                name: game_name(i),
                source: GameSource {
                    id: "seed".to_string(),
                    lookup_id: i.to_string(),
                    external_ids: HashMap::new(),
                },
                cover_url: None,
                summary: Some(format!("Seeded test game #{i}.")),
                description: None,
            })
            .collect();

        inserted += storage.insert_many(&games).expect("failed to insert batch");
        println!("inserted {inserted}/{count}");
    }

    println!("done: {inserted} games seeded (source_id=\"seed\")");
}
