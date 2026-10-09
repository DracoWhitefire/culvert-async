# Roadmap

culvert-async moves in lockstep with culvert: it uses culvert's register map
(`culvert::codec`) and gains the async counterpart of each new culvert method in the same
release. Its open items are therefore culvert's — see culvert's
[`doc/roadmap.md`](https://github.com/DracoWhitefire/culvert/blob/main/doc/roadmap.md):

- **CED checksum (0x56)** — reading and verifying `ERR_DET_Checksum`, together with
  reading the CED block in one transaction through `ScdcTransport::read_block`.
- **DSC status** — the DSC status registers.
- **Manufacturer identification (0xD0–0xDD)** — vendor-defined identification data.

None of these will be added here ahead of culvert.
