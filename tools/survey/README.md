# Hardware survey

Two read-only scripts that record what each machine can do. Neither changes any settings.

| Script | Run on | How |
|---|---|---|
| `linux-survey.sh` | Every Linux machine | `./linux-survey.sh`, then `sudo ./linux-survey.sh --nhi-caps` |
| `mac-survey.sh` | The Mac | `sh mac-survey.sh` |

Each run writes a `tb-survey-*.txt` report in the current directory. Send the reports back (attach them in the project thread, or commit them under `docs/surveys/`), and they get summarized into `docs/HARDWARE.md`.

What the Linux report answers:

- **Is there a USB4/Thunderbolt host interface at all?** See the "PCI: USB4 / Thunderbolt host interfaces" section. If it says "none found", the machine cannot do host-to-host links. A USB root hub at `speed=20000M` is USB 3.2 Gen 2x2, which is not USB4.
- **Which generation, which connection manager, what security level?** See the "Thunderbolt bus" section: `generation` on the host router, `security` on the domain.
- **How many DMA rings does the NHI have?** See `hop_count` in "NHI REG_CAPS" (root run with `--nhi-caps`). If it reads `0xffffffff`, the controller was asleep; plug in a cable and rerun.
- **Can it build a Rust kernel module?** See `CONFIG_RUST` and `rustc` in "Kernel config".

The Mac-to-Linux discovery test is in `docs/PEER-TEST.md`.
