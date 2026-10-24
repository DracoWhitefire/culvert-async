//! The [`Scdc`] client.

use hdmi_hal_async::scdc::ScdcTransport;

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
/// # Cancellation
///
/// To stop early, check a deadline or cancel signal before each call and stop there,
/// rather than dropping a future mid-flight: no transaction is then cut short, whatever
/// the transport does. If a future is dropped, `Scdc<T>` itself stays usable (it keeps no
/// state between calls); the transport's documentation says what a cancelled transfer
/// leaves behind (see `ScdcTransport`'s "Cancellation"). What a dropped call has done is
/// then unknown, method by method:
///
/// - **Writes that set a whole register** — `write_source_version`, `write_tmds_config`,
///   `write_config_0`, `write_frl_config` — may or may not have reached the sink, and are
///   safe to repeat: writing the same value again has the same effect.
/// - **`clear_update_flags`** writes 1 to clear, so repeating it can clear a flag the sink
///   set again in the meantime. For `CED_Update` and `RSED_Update` that is harmless (the
///   sink re-evaluates them about once a second); for a flag you act on, such as
///   `FLT_Update` or `FRL_Start`, read `Update_0` again ([`read_update_flags`]) and clear
///   only what is set, rather than repeating the clear.
/// - **Reads** change nothing on the sink and are read again — except the error counters
///   ([`read_ced`], [`read_rs_correction`]), which a sink may clear when they are read:
///   a dropped read of them can lose counts.
///
/// [`read_update_flags`]: Self::read_update_flags
/// [`read_ced`]: Self::read_ced
/// [`read_rs_correction`]: Self::read_rs_correction
///
/// # Example
///
/// ```rust
/// use culvert_async::Scdc;
/// use hdmi_hal_async::scdc::{ScdcTransport, read_block_bytewise};
///
/// struct StubTransport;
/// impl ScdcTransport for StubTransport {
///     type Error = core::convert::Infallible;
///     async fn read(&self, _reg: u8) -> Result<u8, Self::Error> { Ok(0) }
///     async fn write(&mut self, _reg: u8, _value: u8) -> Result<(), Self::Error> { Ok(()) }
///     async fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), Self::Error> {
///         read_block_bytewise(self, reg, buf).await
///     }
/// }
///
/// # pollster::block_on(async {
/// let mut scdc = Scdc::new(StubTransport);
/// let sink_version = scdc.read_sink_version().await.unwrap();
/// scdc.write_source_version(1).await.unwrap();
/// # });
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
