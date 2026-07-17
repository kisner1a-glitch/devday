//! Background tokio tasks. Each spawn sends its result back as an Event
//! over the channel; the event loop never blocks on collection or posting.

use tokio::sync::mpsc::UnboundedSender;

use crate::config::Config;
use crate::pipeline::{build_report, ReportOptions};
use crate::tui::event::Event;

pub fn spawn_report(tx: UnboundedSender<Event>, cfg: Config, opts: ReportOptions) {
    tokio::spawn(async move {
        let res = build_report(&cfg, &opts).await.map_err(|e| e.to_string());
        let _ = tx.send(Event::ReportReady(res));
    });
}
