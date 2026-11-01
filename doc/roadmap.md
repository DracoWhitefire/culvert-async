# Roadmap

culvert-async moves in lockstep with culvert: it uses culvert's register map
(`culvert::codec`) and gains the async counterpart of each new culvert method in its next
release. Its register items are therefore culvert's — see culvert's
[`doc/roadmap.md`](https://github.com/ashlar-display/culvert/blob/main/doc/roadmap.md):

- **`LIP_Update` (`Update_0` bit 7, HDMI 2.2)** — a field for the flag.
- **`Update_1` (0x11)** — wrapped if a field is ever defined.
- **DSC status** — the DSC status registers.
- **`Test_Config_0` (0xC0)** — the read-request test register, with read request support.
- **Manufacturer identification (0xD0–0xDD)** — vendor-defined identification data.

And culvert's additions to registers it already reads, each additive:

- **One CED and RS read (0x50–0x5A)** — both in one transaction, for a link health monitor;
  designed with the monitor.
- **Read-back decoders for register dumps** — `Config_0`, `Config_1`, `TMDS_Config`.
- **Reading `TMDS_Config` (0x20)** — `read_tmds_config`, for the planned TMDS bring-up
  crate's reconnect check.
- **Raw CED bytes in `CedReading`** — when a consumer needs them.

None of these will be added here ahead of culvert.

## culvert-async's own

- **`SendScdc<T>`** — a second client type implementing plumbob-async's `SendScdcClient`,
  for code bounded by that trait (see `doc/architecture.md`, "`Send` futures"). Its trigger,
  a consumer generic over `SendScdcClient`, is now in sight: the planned link
  orchestrator's async companion drives training generic over the SCDC client, so that a
  third-party client can be used. Planned for culvert-async's release that follows
  hdmi-hal-async's breaking release (below).
- **Following hdmi-hal-async's PHY trait split** — culvert-async re-exports
  hdmi-hal-async's `ScdcTransport` and `SendScdcTransport`, so hdmi-hal-async's planned
  breaking release is a breaking release here too; it carries `SendScdc<T>`.
