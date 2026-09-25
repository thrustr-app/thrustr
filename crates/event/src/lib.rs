//! A simple event system for Thrustr. This is not intended
//! to be a general-purpose event system, but rather a simple way to indicate
//! to the UI that something has changed and it should refresh state.
//!
//! For example, when a plugin is loaded, [`Topic::Plugin`] can be emitted to indicate
//! that the UI should refresh the list of plugins, show notifications, etc.
//!
//! This way the UI and other interfaces can react to external changes without tight coupling.
use dashmap::DashMap;
use std::sync::OnceLock;
use tokio::sync::watch::{Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Topic {
    Component,
    Games,
    Plugin,
}

fn emitter() -> &'static DashMap<Topic, Sender<()>> {
    static MAP: OnceLock<DashMap<Topic, Sender<()>>> = OnceLock::new();
    MAP.get_or_init(DashMap::new)
}

pub fn emit(topic: Topic) {
    if let Some(tx) = emitter().get(&topic) {
        tx.send_replace(());
    }
}

pub fn listen(topic: Topic) -> Receiver<()> {
    if let Some(tx) = emitter().get(&topic) {
        return tx.subscribe();
    }

    emitter()
        .entry(topic)
        .or_insert_with(|| Sender::new(()))
        .subscribe()
}
