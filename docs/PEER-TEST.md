# Mac peer discovery test

Question: will a Thunderbolt 5 Mac offer RDMA over Thunderbolt to a Linux host that is not Thunderbolt 5?

Apple documents RDMA over Thunderbolt for "Macs with Apple silicon with Thunderbolt 5" on macOS 26.2 or later (TN3205). It does not say what happens when the peer is a slower or non-Apple host. This test finds out without writing any code, because the Linux `thunderbolt` driver already reads the peer's XDomain properties and lists every service the peer advertises in sysfs.

## What TN3205 tells us

- RDMA is enabled once, from macOS Recovery: Utilities > Terminal, `rdma_ctl enable`, reboot.
- Each RDMA device is paired with a Thunderbolt IP interface (`rdma_en2` with `en2`), and GIDs are that interface's IP addresses. So the Mac's RDMA rides alongside ThunderboltIP, the same protocol Linux `thunderbolt-net` already speaks.
- A port goes `PORT_ACTIVE` only when the connected Mac also has RDMA enabled. This is the observable that tells us whether the Mac considers the Linux peer RDMA-capable.
- Limits: Send/Receive only, at most 10 UC queue pairs, messages up to 16,773,120 bytes, 4095 outstanding work requests.

## Setup

1. On the Mac, enable RDMA as above. After rebooting, `ibv_devices` should list `rdma_en*` devices.
2. On the Mac, in System Settings > Network, leave Thunderbolt Bridge as it is for now; a single cable cannot form a loop.
3. On the Linux host, check the domain's `security` value in a survey report. `none` or `user` is fine. `secure` or `dponly` may block host-to-host links; change it in the BIOS Thunderbolt settings if so.

## Steps

Do this with the Spectre first (Thunderbolt 3), then with any other Linux machine whose survey shows a USB4 host interface.

1. With nothing connected, run `sh mac-survey.sh before.txt` on the Mac and `sudo ./linux-survey.sh before.txt` on Linux.
2. On Linux, start `sudo dmesg -w | grep -iE 'thunderbolt|xdomain'` in a second terminal.
3. Connect the Mac and the Linux host with a Thunderbolt cable (a Thunderbolt 4 or 5 cable, not a USB-C charging cable).
4. Wait about ten seconds, then run `sudo ./linux-survey.sh --nhi-caps after.txt` on Linux and `sh mac-survey.sh after.txt` on the Mac.
5. On Linux, try the existing IP link as a baseline: `ip -br addr` should show a new `thunderbolt0` interface. Assign addresses on both sides, or use the Mac's 169.254 address, and ping across.

## Reading the result

In the Linux `after.txt`, under "Thunderbolt bus", look for:

- An XDomain entry such as `0-1` with `device_name` and `vendor_name` naming the Mac. If it is missing, the link did not come up as host-to-host at all, and the dmesg output says why.
- Service entries such as `0-1.1`, `0-1.2`. Each has `key`, `prtcid`, `prtcvers`, `prtcrevs`, `prtcstns`. The `network` key is ThunderboltIP. **Any other key is a candidate for Apple's RDMA service**, and its values are what `tb-uc` has to advertise and bind to.

In the Mac `after.txt`, under "RDMA over Thunderbolt", look at the port state of the `rdma_en*` device paired with the Thunderbolt interface that came up.

| Linux sees a non-`network` service | Mac port state | Meaning |
|---|---|---|
| Yes | any | The Mac offers RDMA to this peer. `tb-uc` interop is possible on this hardware; next step is capturing frames. |
| No | `PORT_DOWN` | Either the Mac only advertises RDMA to peers that advertise it back, or only over Thunderbolt 5. Repeat once `tb-uc` can advertise a matching service; if still nothing, a Thunderbolt 5 Linux host is needed. |
| No XDomain entry at all | n/a | Host-to-host link failed. Check the security level and dmesg before drawing conclusions. |

Record the outcome in the matrix in `docs/HARDWARE.md` (rows "Thunderbolt 3" and "USB4", column "`tb-uc` with a Thunderbolt 5 Mac").
