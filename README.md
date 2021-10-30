# Xous Core

Core files for the Xous microkernel operating system.

You might find this [wiki](https://github.com/betrusted-io/betrusted-wiki/wiki) handy, as well as the [Xous Book](https://betrusted.io/xous-book/).

This repository contains everything necessary to build the Xous kernel
from source.  It consists of the following projects:

* **kernel**: core memory manager, irq manager, and syscallhandler
* **loader**: initial loader used to start the kernel
* **tools**: programs used to construct a final boot image
* **docs**: documentation on various aspects of Xous
* **emulation**: Renode scripts used to emulate Xous
* **xous-rs**: userspace library

## Dependencies

- Xous requires its own Rust target, `riscv32imac-unknown-xous-elf`. If you run `cargo xtask` from the command line, you should be prompted to install the target automatically if it does not already exist.
- You may need to remove the `target/` directory before building, if `rustc` continues to behave like it can't find the `xous` target even after it is installed.
- If you plan on doing USB firmware updates, you'll need `progressbar2` (updates) and `pyusb` (updates). Note that `pyusb` has name space conflicts with similarly named packages, so if updates aren't working you may need to create a `venv` or uninstall conflicting packages.
- If you are doing development on the digital signatures with the Python helper scripts, you will need: `pycryptodome` (signing - PEM read), `cryptography` (signing - x509 read), `pynacl` (signing - ed25519 signatures) (most users won't need this).

## Quickstart using Hosted Mode

You can try out Xous in a "hosted mode" wherein programs are compiled
for your native platform and are run locally as processes within your
current operating system. System calls are replaced with network calls
to a kernel that simply shuffles messages around.

Xous uses the [xtask](https://github.com/matklad/cargo-xtask/) convention,
where various complex build commands are stored under `cargo xtask`.
This allows for us to create arbitrarily complex build sequences
without resorting to `make` (which is platform-dependent),
`sh` (which requires a lot of external tooling), or another build
system.

To build a set of sample programs and run them all using the
kernel for communication, clone this repository and run:

```sh
cargo xtask run
```

This will build several servers and a "shell" program to control them
all. Most notably, a `graphics-server` will appear and kernel messages
will begin scrolling in your terminal.

## Quickstart using an emulator

Xous uses [Renode](https://renode.io/) as the preferred emulator, because
it is easy to extend the hardware peripherals without recompiling the
entire emulator.

[Download Renode](https://renode.io/#downloads) and ensure it is in your path.
Then, build Xous:

```sh
cargo xtask renode-image
```

This will compile everything in `release` mode for RISC-V, compile the tools
require to package it all up, then create an image file.

Finally, run Renode and specify the `xous-release.resc` REnode SCript:

```sh
renode emulation/xous-release.resc
```

Renode will start emulation automatically, and will run the same set of programs
as in "Hosted mode".

## Generating a hardware image

To build for real hardware, you must specify an `.svd` file. This
file is generated by the SoC build process and describes a single
Betrusted core. These addresses will change as hardware is modified,
so if you distribute a modified Betrusted core, you should be sure
to distribute the `.svd` file.

We have included a reference version of the gateware and its SVD
file in the `precursors` directory, so you can compile a gateware
for the reference image using this command:

```sh
cargo xtask hw-image precursors/soc.svd
```

The resulting images are in your target directory (typically `target/riscv32imac-unknown-xous-elf/release/`)
with the names `xous.img` (for the kernel) and `loader.bin` (for its bootloader). The corresponding
gateware is in `precursors/soc_csr.bin`. These can be written to your
device by following the [update guide](https://github.com/betrusted-io/betrusted-wiki/wiki/Updating-Your-Device).
