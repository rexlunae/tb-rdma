#!/usr/bin/env bash
# Read-only survey of a Linux host's USB4/Thunderbolt hardware for tb-rdma.
#
# Usage: linux-survey.sh [--nhi-caps] [OUTFILE]
#
# Run it as a normal user first. Run it again with sudo to add dmesg lines,
# and with --nhi-caps (root only) to read the NHI ring count from REG_CAPS.
# The report goes to OUTFILE (default tb-survey-<host>-<date>.txt) and stdout.
# Nothing is written to sysfs, PCI config space or MMIO.

set -u

NHI_CAPS=0
OUT=""
for arg in "$@"; do
	case "$arg" in
	--nhi-caps) NHI_CAPS=1 ;;
	-h|--help) sed -n '2,11p' "$0"; exit 0 ;;
	*) OUT="$arg" ;;
	esac
done
[ -n "$OUT" ] || OUT="tb-survey-$(hostname -s 2>/dev/null || echo host)-$(date +%Y%m%d-%H%M%S).txt"

exec > >(tee "$OUT") 2>&1

section() { printf '\n===== %s =====\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }
readattr() { [ -r "$1" ] && tr -d '\0' < "$1" 2>/dev/null | head -c 512; }

section "System"
echo "survey-version: 1"
echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "user: $(id -un) (uid $(id -u))"
echo "hostname: $(hostname 2>/dev/null)"
uname -a
[ -r /etc/os-release ] && grep -E '^PRETTY_NAME=' /etc/os-release
grep -m1 -E 'model name|^Hardware|^Model' /proc/cpuinfo 2>/dev/null
for f in sys_vendor product_name board_vendor board_name bios_version bios_date; do
	v=$(readattr "/sys/class/dmi/id/$f") && [ -n "$v" ] && echo "dmi.$f: $v"
done
[ -r /proc/device-tree/model ] && echo "devicetree.model: $(tr -d '\0' < /proc/device-tree/model)"

section "Kernel config"
CFG=""
if [ -r /proc/config.gz ]; then CFG="zcat /proc/config.gz"
elif [ -r "/boot/config-$(uname -r)" ]; then CFG="cat /boot/config-$(uname -r)"
fi
if [ -n "$CFG" ]; then
	echo "source: $CFG"
	$CFG | grep -E '^(# )?CONFIG_(RUST|THUNDERBOLT|USB4|USB4_NET|USB4_DEBUGFS_WRITE|USB4_DMA_TEST|THUNDERBOLT_NET|INFINIBAND|INFINIBAND_USER_ACCESS|RDMA_RXE|IOMMU_SUPPORT|AMD_IOMMU|INTEL_IOMMU|ARM_SMMU_V3)[= ]'
else
	echo "kernel config not found (/proc/config.gz or /boot/config-$(uname -r))"
fi
have rustc && echo "rustc: $(rustc --version)"

section "PCI: USB4 / Thunderbolt host interfaces (NHI)"
# USB4 host interfaces use class 0x0c0340. Pre-USB4 Thunderbolt NHIs use other
# classes, so also match anything bound to the thunderbolt driver, anything
# whose modalias resolves to it (works when it is built but not bound), and
# lspci names mentioning an NHI (works when it is not built at all).
is_nhi() {
	local d=$1 class=$2 drv=$3
	[ "$class" = "0x0c0340" ] || [ "$drv" = "thunderbolt" ] && return 0
	if have modprobe && [ -r "$d/modalias" ]; then
		modprobe -R "$(cat "$d/modalias")" 2>/dev/null | grep -qx thunderbolt && return 0
	fi
	have lspci && lspci -s "$(basename "$d")" 2>/dev/null | grep -qiE 'thunderbolt.*nhi|usb4 host|usb4 nhi' && return 0
	return 1
}
found=0
for d in /sys/bus/pci/devices/*; do
	class=$(readattr "$d/class")
	drv=""; [ -e "$d/driver" ] && drv=$(basename "$(readlink "$d/driver")")
	if is_nhi "$d" "$class" "$drv"; then
		found=1
		slot=$(basename "$d")
		echo "$slot class=$class vendor=$(readattr "$d/vendor") device=$(readattr "$d/device") driver=${drv:-none} power=$(readattr "$d/power/runtime_status")"
		have lspci && lspci -nn -s "$slot" | sed 's/^/    /'
	fi
done
if [ "$found" = 0 ]; then
	if have lspci; then
		echo "none found: no USB4/Thunderbolt host interface on this machine"
	else
		echo "none found, but inconclusive: without lspci, controllers the running"
		echo "kernel does not recognize are missed; install pciutils and rerun"
	fi
fi

section "PCI: all USB controllers"
if have lspci; then lspci -nn | grep -iE 'USB controller|Thunderbolt|USB4' || echo "none"
else echo "lspci not installed (pciutils)"; fi

section "USB root hub speeds"
# 20000 = USB 3.2 Gen 2x2 (20 Gb/s), which is not USB4 and cannot do host-to-host.
for h in /sys/bus/usb/devices/usb*; do
	[ -e "$h" ] || continue
	pci=$(basename "$(readlink -f "$h/..")")
	echo "$(basename "$h") speed=$(readattr "$h/speed")M product=\"$(readattr "$h/product")\" controller=$pci"
done
have lsusb && { echo; lsusb -t; }

section "Thunderbolt bus"
TB=/sys/bus/thunderbolt/devices
if [ -d "$TB" ]; then
	attrs="security iommu_dma_protection deauthorization boot_acl authorized boot generation \
vendor vendor_name device device_name unique_id maxhopid rx_speed rx_lanes tx_speed tx_lanes \
nvm_version key prtcid prtcvers prtcrevs prtcstns modalias link offline"
	for e in "$TB"/*; do
		[ -e "$e" ] || { echo "bus present, no devices"; break; }
		type=$(grep -m1 '^DEVTYPE=' "$e/uevent" 2>/dev/null | cut -d= -f2)
		echo "--- $(basename "$e") (${type:-unknown})"
		for a in $attrs; do
			v=$(readattr "$e/$a") && [ -n "$v" ] && echo "    $a: $v"
		done
	done
else
	echo "/sys/bus/thunderbolt does not exist (thunderbolt driver not loaded or not built)"
fi
have boltctl && { echo; echo "boltctl list:"; boltctl list 2>&1; }

section "Modules"
lsmod 2>/dev/null | grep -E '^(thunderbolt|thunderbolt_net|ib_core|ib_uverbs|rdma_rxe|rdma_ucm)\b' || echo "none of thunderbolt, thunderbolt_net, ib_* loaded"

section "Network interfaces"
for n in /sys/class/net/*; do
	drv=""; [ -e "$n/device/driver" ] && drv=$(basename "$(readlink "$n/device/driver")")
	echo "$(basename "$n") driver=${drv:-virtual} state=$(readattr "$n/operstate") mtu=$(readattr "$n/mtu")"
done

section "RDMA devices"
if [ -d /sys/class/infiniband ] && [ -n "$(ls /sys/class/infiniband 2>/dev/null)" ]; then
	ls /sys/class/infiniband
	have rdma && rdma link 2>&1
else
	echo "none"
fi

section "Kernel log (thunderbolt)"
if dmesg >/dev/null 2>&1; then
	dmesg | grep -iE 'thunderbolt|usb4|nhi|xdomain' | tail -n 120
else
	echo "dmesg not readable as $(id -un); rerun with sudo to include it"
fi

if [ "$NHI_CAPS" = 1 ]; then
	section "NHI REG_CAPS"
	# drivers/thunderbolt/nhi.c: hop_count = ioread32(iobase + REG_CAPS) & 0x3ff,
	# REG_CAPS = 0x39640 in BAR0 (nhi_regs.h). One 32-bit MMIO read, no writes.
	if [ "$(id -u)" != 0 ]; then
		echo "skipped: --nhi-caps needs root"
	elif ! have python3; then
		echo "skipped: needs python3"
	else
		for d in /sys/bus/pci/drivers/thunderbolt/*:*; do
			[ -e "$d/resource0" ] || continue
			python3 -I - "$d/resource0" <<'PY'
import mmap, os, struct, sys
path = sys.argv[1]
REG_CAPS = 0x39640
fd = os.open(path, os.O_RDONLY | os.O_SYNC)
try:
    page = REG_CAPS & ~(mmap.PAGESIZE - 1)
    m = mmap.mmap(fd, mmap.PAGESIZE, mmap.MAP_SHARED, mmap.PROT_READ, offset=page)
    val = struct.unpack_from("<I", m, REG_CAPS - page)[0]
    m.close()
finally:
    os.close(fd)
slot = path.split("/")[-2]
if val == 0xFFFFFFFF:
    print(f"{slot}: read 0xffffffff (controller is runtime-suspended; plug in a cable and rerun)")
else:
    print(f"{slot}: REG_CAPS=0x{val:08x} hop_count={val & 0x3ff} version=0x{(val >> 16) & 0xff:02x}")
PY
		done
	fi
fi

echo
echo "Report written to $OUT"
