use std::{
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

use log::info;
use s2n_quic::provider::event::{self, events::RecoveryMetrics, ConnectionMeta};

pub struct RecoveryMetricsLogger {
    logfile_writer: Box<dyn Write + Send>,
}

pub struct IPAConnectionContext {}

impl RecoveryMetricsLogger {
    pub fn new(mut logfile_writer: Box<dyn Write + Send>) -> Self {
        let header_fields = [
            "time",
            "conn_id",
            "min_rtt",
            "smoothed_rtt",
            "latest_rtt",
            "rtt_variance",
            "max_ack_delay",
            "pto_count",
            "congestion_window",
            "bytes_in_flight",
        ];

        writeln!(logfile_writer, "{}", header_fields.join(",")).unwrap();

        Self { logfile_writer }
    }
}

impl event::Subscriber for RecoveryMetricsLogger {
    type ConnectionContext = IPAConnectionContext;

    fn create_connection_context(
        &mut self,
        _meta: &event::ConnectionMeta,
        _info: &event::ConnectionInfo,
    ) -> Self::ConnectionContext {
        return IPAConnectionContext {};
    }

    fn on_recovery_metrics(
        &mut self,
        _context: &mut Self::ConnectionContext,
        meta: &ConnectionMeta,
        event: &RecoveryMetrics,
    ) {
        let now = SystemTime::now();
        let nanos_since_unix = now.duration_since(UNIX_EPOCH).unwrap();

        writeln!(
            self.logfile_writer,
            "{},{},{},{},{},{},{},{},{},{}",
            nanos_since_unix.as_nanos(),
            meta.id,
            event.min_rtt.as_nanos(),
            event.smoothed_rtt.as_nanos(),
            event.latest_rtt.as_nanos(),
            event.rtt_variance.as_nanos(),
            event.max_ack_delay.as_nanos(),
            event.pto_count,
            event.congestion_window,
            event.bytes_in_flight
        )
        .unwrap();
    }
}

impl Drop for RecoveryMetricsLogger {
    fn drop(&mut self) {
        self.logfile_writer.flush().unwrap();
        info!("Flushed logfile writer.");
    }
}
