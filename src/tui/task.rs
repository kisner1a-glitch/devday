//! Background tokio tasks. Each spawn sends its result back as an Event
//! over the channel; the event loop never blocks on collection or posting.
