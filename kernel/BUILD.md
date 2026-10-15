# Building and loading the tb_rdma kernel module (v0.1)

Status: validated working on one machine. See the end for what v0.1 does and
does not do, and for the two machines used to validate.

## What v0.1 is

`kernel/` now contains a buildable, loadable out-of-tree Linux module:

- `tb_rdma_core.rs` — crate root. Includes `crates/tb-rdma-proto/src/lib.rs`
  unchanged via `#[path]` (kernel/README.md step 3), runs the codec selftest
  at init, then calls the C glue.
- `selftest.rs` — in-kernel port of the host test suite (10 groups: sizes,
  byte order, Send/Write roundtrips, RETH mismatch, strict decode, 2000-iter
  fuzz, UC profile, credits, PSN wrap/verdicts). Failure refuses module load.
- `credit.rs`, `qp.rs` — `core`-only ports of the `tb-rdma-core` pieces that
  need no allocation (CreditPool, type-state QP). The std collections in
  `route.rs`/`vring.rs` are NOT ported yet (they need `kernel` rbtree/list or
  the NHI rings; tracked in kernel/README.md).
- `tb_rdma_glue.c` — C-side Thunderbolt glue. `struct tb_service_driver` and
  the property API are not stable enough across kernels to bind from Rust
  without `tb_rdma_sys` bindings, so the C object does
  `tb_property_create_dir("tb-rdma")` + `tb_register_property_dir` +
  `tb_register_service_driver` and calls the `#[no_mangle]` Rust functions.
  Both objects link into one `.ko`; nothing crosses a module boundary.

## Kernel requirements (per host)

An out-of-tree RUST module needs, from the kernel you will load it on:

1. `CONFIG_RUST=y` and the **generated Rust artifacts shipped in the headers**
   (`rust/libcore.rmeta`, `rust/libkernel.rmeta`, `rust/libbindings.rmeta`,
   plus `libmacros.so`, `libpin_init*.rmeta`). Ubuntu's linux-headers packages
   ship NONE of these (only a `rust/Makefile` stub) even when the running
   kernel is Rust-enabled; Arch's do. Check with:
       ls /lib/modules/$(uname -r)/build/rust/libkernel.rmeta
2. A `rustc` whose version string byte-for-byte matches the compiler that
   produced those `.rmeta` files, or the build fails with E0514
   ("found crate `core` compiled by an incompatible version of rustc").
   rustup's build of the same version is NOT compatible with a distro build.
   Arch: install the `rust` package (not rustup's shim). Ubuntu: you need a
   kernel you built yourself (see below).
3. `bindgen` + `libclang` if the build regenerates bindings (not needed for
   this module as long as the rmeta files are shipped).

## Recipes that worked

### erskin (CachyOS install, Arch kernel 7.2.9)

The running kernel was linux-cachyos 7.2.8 (ThinLTO build, `CONFIG_RUST` is
off by design there — see the CachyOS forum; linux-cachyos-gcc no longer
exists in the repo). Arch's `linux` package DOES have `CONFIG_RUST=y`:

    sudo pacman -S linux linux-headers kexec-tools rust   # rust = distro rustc
    # reboot into the arch kernel (or kexec: extract the UKI's .initrd with
    # objcopy --only-section=.initrd and kexec -l with your root= cmdline)
    cd kernel
    make KDIR=/lib/modules/7.2.9-arch1-1/build
    sudo insmod tb_rdma.ko

kexec notes: the arch package installs a UKI (no plain initrd file); pull the
initrd out of the .efi as above and pass your own root= cmdline. On a
Limine+rEFInd machine, rEFInd auto-discovers both kernels, so a failed boot
is recoverable from the boot menu.

### kleya (Ubuntu 26.04, Secure Boot, lockdown integrity)

Ubuntu ships `CONFIG_RUST=y` but no Rust rmeta in the headers, so build
against the kernel source tree (`linux-source-7.0.0`) instead:

    sudo apt install bison flex libssl-dev libelf-dev libclang-dev clang bc \
        dwarves libdw-dev
    # Ubuntu's rust/Makefile hardcodes /usr/src/rustc-<version>/library;
    # point it at your toolchain sources:
    rustup component add rust-src
    sudo ln -sfn ~/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/src/rust \
        /usr/src/rustc-1.98.1
    tar xf /usr/src/linux-source-7.0.0.tar.bz2
    cp /boot/config-$(uname -r) linux-source-7.0.0/.config
    sed -i 's|CONFIG_SYSTEM_TRUSTED_KEYS="debian/canonical-certs.pem"|CONFIG_SYSTEM_TRUSTED_KEYS=""|;s|CONFIG_SYSTEM_REVOCATION_KEYS="debian/canonical-revoked-certs.pem"|CONFIG_SYSTEM_REVOCATION_KEYS=""|' \
        linux-source-7.0.0/.config
    # olddefconfig DROPS CONFIG_RUST unless the toolchain check passes, so:
    make -j32 rustavailable        # must say "Rust is available!"
    ./scripts/config -e RUST
    make -j32 olddefconfig
    make -j32 all                  # vmlinux + modules + rust artifacts

Module for the RUNNING kernel: the tree's utsrelease says 7.0.14 while the
running kernel's vermagic is `7.0.0-38-generic` (Ubuntu keeps the base name;
the source tree is newer). To match:

    echo 7.0.0-38-generic | sudo tee include/config/kernel.release
    echo '#define UTS_RELEASE "7.0.0-38-generic"' | sudo tee include/generated/utsrelease.h
    make -C kernel KDIR=$PWD        # vermagic now matches; CRCs come from the
                                    # same source package, so they match too

Signing: under lockdown integrity the module needs a key the kernel trusts.
Either enroll a MOK (`mokutil --import`, one blue-screen password prompt at
next boot, then `scripts/sign-file sha512 key.pem cert.der module.ko`), or
boot the kernel you built yourself — its build key (`certs/signing_key.pem`)
is in its own trusted keyring, so modules signed with `sign-file` against
`certs/signing_key.x509` load with no MOK dance. That is the path used here:
`make modules_install` (needs root; installs under the tree's own release,
7.0.14, so it cannot clobber /lib/modules/7.0.0-38-generic),
`update-initramfs -c -k 7.0.14`, then `kexec -l arch/x86/boot/bzImage
--initrd /boot/initrd.img-7.0.14 --append "root=UUID=... rw panic=10"`.

## What v0.1 does and does not do

Does: load on a Rust-enabled kernel; self-test the included proto codec in
kernel; advertise a `tb-rdma` XDomain service (prtcid/prtcvers/prtcrevs 1)
and bind a service driver on the thunderbolt bus, so a peer's discovery now
sees us offering RDMA.

Does not (yet): allocate NHI rings, speak the wire protocol over an
XDomain DMA path, or expose a uverbs-compatible ABI. kernel/README.md's
"Order of work" step 4 (kernel-crate state rewrite) is next.

## Validation performed

- erskin 7.2.9-arch1-1: `insmod` OK; all 10 selftest groups pass in dmesg;
  `tb-rdma` appears in `/sys/bus/thunderbolt/drivers/`; `rmmod` clean.
- kleya (ASM4242 USB4 NHI): module builds against the tree with matching
  vermagic and correct CRCs; load waits on the MOK enroll or a kexec/boot of
  the built kernel (both staged).

## Environment bugs found on the way (filed separately)

- tools/survey/linux-survey.sh --nhi-caps cannot read REG_CAPS on a
  Secure Boot machine (mmap of resource0 is blocked by lockdown integrity,
  even as root) and crashes with a traceback instead of saying so.
- kernel/README.md's `#[path]` include plan needs the 1-line core prelude
  import added to tb-rdma-proto/src/lib.rs (module files do not get the
  core prelude injection; see the patch in the kernel-mod branch).
