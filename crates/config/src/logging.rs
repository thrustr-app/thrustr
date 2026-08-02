use crate::paths;
use chrono::Local;
use std::{backtrace::Backtrace, cmp::Reverse, fs, panic, path::Path};
use tracing::error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    EnvFilter, filter::LevelFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

const LOG_FILE_PREFIX: &str = "thrustr";
const LOG_FILE_SUFFIX: &str = ".log";
const MAX_LOG_FILES: usize = 10;

pub fn init() -> WorkerGuard {
    let dir = paths::logs_dir();
    fs::create_dir_all(&dir).expect("application logs directory should be creatable");

    prune_old_logs(&dir, MAX_LOG_FILES - 1);

    let started_at = Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{LOG_FILE_PREFIX}-{started_at}{LOG_FILE_SUFFIX}");

    let file_appender = tracing_appender::rolling::never(&dir, filename);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    let file_layer = fmt::layer().with_ansi(false).with_writer(non_blocking);

    let stderr_layer = cfg!(debug_assertions).then(|| fmt::layer().with_writer(std::io::stderr));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(stderr_layer)
        .init();

    install_panic_hook();

    guard
}

fn install_panic_hook() {
    let previous = panic::take_hook();

    panic::set_hook(Box::new(move |info| {
        let location = info.location().map(ToString::to_string);
        let backtrace = Backtrace::force_capture();

        error!(
            reason = info.payload_as_str().unwrap_or("unknown"),
            location = location.as_deref().unwrap_or("unknown"),
            %backtrace,
            "panicked"
        );

        previous(info);
    }));
}

fn prune_old_logs(dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    let mut logs: Vec<_> = entries
        .flatten()
        .filter(|entry| {
            entry.file_name().to_str().is_some_and(|name| {
                name.starts_with(LOG_FILE_PREFIX) && name.ends_with(LOG_FILE_SUFFIX)
            })
        })
        .filter_map(|entry| {
            let modified = entry.metadata().and_then(|m| m.modified()).ok()?;
            Some((entry.path(), modified))
        })
        .collect();

    if logs.len() <= keep {
        return;
    }

    logs.sort_by_key(|(_, modified)| Reverse(*modified));
    for (path, _) in logs.into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}
