//! A simple event system for Thrustr. This is not intended
//! to be a general-purpose event system, but rather a simple way to indicate
//! to the UI that something has changed and it should refresh state.
//!
//! For example, when a plugin is loaded, [`Topic::Plugin`] can be emitted to indicate
//! that the UI should refresh the list of plugins, show notifications, etc.
//!
//! This way the UI and other interfaces can react to external changes without tight coupling.
use async_broadcast::{InactiveReceiver, Receiver, Sender, broadcast};
use dashmap::DashMap;
use std::sync::OnceLock;

// TODO: investigate replacing with Tokio broadcast channel.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Topic {
    Component,
    Games,
    Plugin,
}

struct EventChannel {
    tx: Sender<()>,
    _anchor: InactiveReceiver<()>,
}

fn emitter() -> &'static DashMap<Topic, EventChannel> {
    static MAP: OnceLock<DashMap<Topic, EventChannel>> = OnceLock::new();
    MAP.get_or_init(DashMap::new)
}

pub fn emit(topic: Topic) {
    if let Some(channel) = emitter().get(&topic) {
        let _ = channel.tx.try_broadcast(());
    }
}

pub fn listen(topic: Topic) -> Receiver<()> {
    if let Some(channel) = emitter().get(&topic) {
        return channel.tx.new_receiver();
    }

    emitter()
        .entry(topic)
        .or_insert_with(|| {
            let (mut tx, rx) = broadcast::<()>(128);
            tx.set_overflow(true);
            EventChannel {
                tx,
                _anchor: rx.deactivate(),
            }
        })
        .tx
        .new_receiver()
}
