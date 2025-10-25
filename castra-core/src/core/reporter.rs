use super::events::Event;

/// Trait implemented by callers that wish to observe progress events emitted by Castra operations.
pub trait Reporter {
    /// Receive a structured event.
    fn report(&mut self, event: Event);
}

impl Reporter for () {
    fn report(&mut self, _event: Event) {}
}
