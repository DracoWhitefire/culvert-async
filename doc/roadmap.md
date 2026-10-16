# Roadmap

culvert-async moves in lockstep with culvert: it uses culvert's register map
(`culvert::codec`) and gains the async counterpart of each new culvert method in the same
release. Its open items are therefore culvert's — see culvert's
[`doc/roadmap.md`](https://github.com/DracoWhitefire/culvert/blob/main/doc/roadmap.md):

- **`LIP_Update` (`Update_0` bit 7, HDMI 2.2)** — a field for the flag.
- **`Update_1` (0x11)** — wrapped if a field is ever defined.
- **CED checksum (0x56)** — reading and verifying `ERR_DET_Checksum`, together with
  reading the CED block in one transaction through `ScdcTransport::read_block`.
- **DSC status** — the DSC status registers.
- **`Test_Config_0` (0xC0)** — the read-request test register, with read request support.
- **Manufacturer identification (0xD0–0xDD)** — vendor-defined identification data.

None of these will be added here ahead of culvert.
