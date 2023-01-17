# Retina-regex

This repository is a fork of [Retina](https://github.com/stanford-esrg/retina) with additional features and bug fixes. Specifically:
**Added features:**
* Parsing of an arbitrary number of VLAN headers.
* Dynamic tracking of filtered flows using a 'lockless' hashmap.
* Dynamic updating regexes, each core has thread-local copies.
* Saving of packets on disk. This can be updated to include sending to other ethernet devices.

**Removed features:**
* Connection tracking and reassembly.
* Static filter compilation.

**Fixed bugs:**
* Segementation faults in logger interface.
* Wrongly initialised RSS hash for queues.
* VLAN headers were wrongly stripped.

## Documentation

For the original paper on Retina see: *[Retina: Analyzing 100 GbE Traffic on Commodity
Hardware](https://thegwan.github.io/files/retina.pdf)*.

To generate the API documentation, you can run `cargo doc` to generata the documentation.

## Getting Started

Install [Rust](https://www.rust-lang.org/tools/install) and
[DPDK](http://core.dpdk.org/download/). Detailed instructions can be found in
[INSTALL](INSTALL.md).

Add `$DPDK_PATH/lib/x86_64-linux-gnu` to your `LD_LIBRARY_PATH`, where `DPDK_PATH` points to the DPDK installation directory.

Fork or clone the main git repository.

## Development
Before every commit run: `cargo fmt` and `cargo clippy`.

## Acknowledgements

The author of this fork is Lucas Crijns.
The original authors of Retina can be found in the [Retina](https://github.com/stanford-esrg/retina) repository.
