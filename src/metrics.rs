use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::protocol::StatsSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Get,
    Set,
    Delete,
    Exists,
    Stats,
}

#[derive(Debug)]
pub struct Metrics {
    started_at: Instant,
    active_connections: AtomicU64,
    total_connections: AtomicU64,
    total_requests: AtomicU64,
    total_errors: AtomicU64,
    get_commands: AtomicU64,
    set_commands: AtomicU64,
    delete_commands: AtomicU64,
    exists_commands: AtomicU64,
    stats_commands: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            active_connections: AtomicU64::new(0),
            total_connections: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            get_commands: AtomicU64::new(0),
            set_commands: AtomicU64::new(0),
            delete_commands: AtomicU64::new(0),
            exists_commands: AtomicU64::new(0),
            stats_commands: AtomicU64::new(0),
        }
    }

    pub fn connection_opened(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
        self.total_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn connection_closed(&self) {
        let _ =
            self.active_connections
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    Some(current.saturating_sub(1))
                });
    }

    pub fn record_request(&self, kind: CommandKind) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        let counter = match kind {
            CommandKind::Get => &self.get_commands,
            CommandKind::Set => &self.set_commands,
            CommandKind::Delete => &self.delete_commands,
            CommandKind::Exists => &self.exists_commands,
            CommandKind::Stats => &self.stats_commands,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn active_connections(&self) -> u64 {
        self.active_connections.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self, key_count: usize) -> StatsSnapshot {
        StatsSnapshot {
            uptime_ms: u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            total_connections: self.total_connections.load(Ordering::Relaxed),
            total_requests: self.total_requests.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
            get_commands: self.get_commands.load(Ordering::Relaxed),
            set_commands: self.set_commands.load(Ordering::Relaxed),
            delete_commands: self.delete_commands.load(Ordering::Relaxed),
            exists_commands: self.exists_commands.load(Ordering::Relaxed),
            stats_commands: self.stats_commands.load(Ordering::Relaxed),
            key_count: u64::try_from(key_count).unwrap_or(u64::MAX),
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandKind, Metrics};

    #[test]
    fn snapshot_reports_connections_requests_errors_and_commands() {
        let metrics = Metrics::new();
        metrics.connection_opened();
        metrics.connection_opened();
        metrics.connection_closed();
        metrics.record_request(CommandKind::Get);
        metrics.record_request(CommandKind::Set);
        metrics.record_error();

        let snapshot = metrics.snapshot(7);
        assert_eq!(snapshot.active_connections, 1);
        assert_eq!(snapshot.total_connections, 2);
        assert_eq!(snapshot.total_requests, 2);
        assert_eq!(snapshot.total_errors, 1);
        assert_eq!(snapshot.get_commands, 1);
        assert_eq!(snapshot.set_commands, 1);
        assert_eq!(snapshot.key_count, 7);
    }
}
