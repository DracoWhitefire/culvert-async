//! The [`Scdc`] client.

use hdmi_hal_async::scdc::ScdcTransport;

mod address;
mod ced;
mod frl;
mod scrambling;
mod update;
mod version;

#[cfg(feature = "plumbob-async")]
mod plumbob_client;

#[cfg(test)]
mod test_transport;

/// Async typed client for the HDMI 2.1 SCDC (Status and Control Data Channel) register map.
///
/// `Scdc<T>` wraps an async [`ScdcTransport`] and exposes one typed `async fn` per register
/// group. It holds no protocol state; all sequencing logic belongs in the caller.
///
/// # Example
///
/// ```ignore
/// let mut scdc = Scdc::new(transport);
/// let sink_version = scdc.read_sink_version().await?;
/// scdc.write_source_version(1).await?;
/// ```
pub struct Scdc<T> {
    transport: T,
}

impl<T: ScdcTransport> Scdc<T> {
    /// Creates a new `Scdc` client wrapping the given async transport.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Consumes the client and returns the underlying transport.
    pub fn into_transport(self) -> T {
        self.transport
    }
}
