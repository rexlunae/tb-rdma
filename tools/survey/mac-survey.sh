#!/bin/sh
# Read-only survey of a Mac's Thunderbolt and RDMA state for tb-rdma.
#
# Usage: sh mac-survey.sh [OUTFILE]
#
# Run once with nothing plugged into the Thunderbolt ports, and again with the
# Linux host connected. The report goes to OUTFILE (default
# tb-survey-mac-<date>.txt) and stdout. It changes no settings.

OUT="${1:-tb-survey-mac-$(date +%Y%m%d-%H%M%S).txt}"

{
section() { printf '\n===== %s =====\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }

section "System"
echo "survey-version: 1"
echo "date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
sw_vers
echo "hw.model: $(sysctl -n hw.model)"
echo "cpu: $(sysctl -n machdep.cpu.brand_string 2>/dev/null)"

section "Thunderbolt ports and connected devices"
system_profiler SPThunderboltDataType 2>&1

section "RDMA over Thunderbolt (TN3205)"
for t in rdma_ctl ibv_devices ibv_devinfo; do
	if have "$t"; then echo "$t: $(command -v "$t")"; else echo "$t: not found"; fi
done
# rdma_ctl can only enable RDMA from macOS Recovery; status may work here.
have rdma_ctl && { echo; echo "rdma_ctl status:"; rdma_ctl status 2>&1; }
have ibv_devices && { echo; ibv_devices 2>&1; }
have ibv_devinfo && { echo; ibv_devinfo -v 2>&1; }

section "Thunderbolt network interfaces"
networksetup -listallhardwareports 2>&1 | grep -A2 -i thunderbolt
echo
ifconfig bridge0 2>&1
for i in $(networksetup -listallhardwareports 2>/dev/null | awk '/Thunderbolt/{getline; print $2}'); do
	case "$i" in en*) echo; ifconfig "$i" 2>&1 ;; esac
done

section "IORegistry (Thunderbolt / RDMA / XDomain lines)"
ioreg -l -w0 2>/dev/null | grep -iE 'rdma|xdomain|ThunderboltIP|IOThunderboltPort|Link Speed|Link Width' | head -n 300

echo
echo "Report written to $OUT"
} 2>&1 | tee "$OUT"
