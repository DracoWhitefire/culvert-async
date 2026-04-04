use hdmi_hal_async::scdc::ScdcTransport;

use crate::ScdcError;

use super::Scdc;
use super::address;

impl<T: ScdcTransport> Scdc<T> {
    /// Reads the sink's SCDC protocol version from `Sink_Version` (0x01).
    pub async fn read_sink_version(&mut self) -> Result<u8, ScdcError<T::Error>> {
        self.transport
            .read(address::SINK_VERSION)
            .await
            .map_err(ScdcError::Transport)
    }

    /// Writes the source's SCDC protocol version to `Source_Version` (0x02).
    pub async fn write_source_version(&mut self, version: u8) -> Result<(), ScdcError<T::Error>> {
        self.transport
            .write(address::SOURCE_VERSION, version)
            .await
            .map_err(ScdcError::Transport)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::TestTransport;

    #[pollster::test]
    async fn read_sink_version_returns_register_value() {
        let mut sim = TestTransport::new();
        sim.set(0x01, 0x01);
        assert_eq!(Scdc::new(sim).read_sink_version().await.unwrap(), 0x01);
    }

    #[pollster::test]
    async fn write_source_version_writes_register() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_source_version(0x01).await.unwrap();
        assert_eq!(scdc.into_transport().get(0x02), 0x01);
    }

    #[pollster::test]
    async fn transport_error_propagates() {
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_sink_version()
                .await
                .is_err()
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .write_source_version(1)
                .await
                .is_err()
        );
    }
}
