mod client;
mod endpoints;
pub mod error;
pub mod models;

pub use client::*;

const BASE_URL: &str = "https://api.legacygames.com";
