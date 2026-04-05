# Roadmap

culvert-async moves in lockstep with culvert: when culvert gains a method for a new
register group, culvert-async gains its async mirror in the same release. The items below
track culvert's open register coverage. None will be added here ahead of culvert.

---

## RS Correction counters (0x58–0x5F)

FRL mode uses Reed-Solomon forward error correction on each lane. The `Rs_Correction`
registers mirror the `ERR_DET` layout: four low/high byte pairs, each holding a 15-bit
counter with a validity bit in the high byte's bit 7.

Future API surface:

```rust
impl<T: hdmi_hal_async::scdc::ScdcTransport> Scdc<T> {
    pub async fn read_rs_correction(
        &mut self,
    ) -> Result<RsCorrectionCounters, ScdcError<T::Error>>;
}
```

The return type `RsCorrectionCounters` is defined in `culvert` and re-exported here,
parallel to `CedCounters`. The implementation is a direct async mirror of culvert's
`read_rs_correction`.

---

## DSC status

`Update_1` bit 0 (`dsc_update`) notifies the source that DSC (Display Stream Compression)
status has changed. culvert-async surfaces this flag via `UpdateFlags` (re-exported from
`culvert`) but provides no method to read the corresponding DSC status registers.

These will be wrapped once culvert wraps them, at which point culvert-async will expose
async mirrors with the same signature shape as the sync versions.

---

## Manufacturer identification (0xC0–0xDD)

HDMI 2.1 defines a range of SCDC registers for sink manufacturer OUI, device
identification, and manufacturer-specific data. These are not required for link training
and are deferred indefinitely, consistent with culvert. If they are ever added, they will
be exposed as a `read_manufacturer_info` method returning raw bytes rather than typed
fields, since the content is vendor-defined.
