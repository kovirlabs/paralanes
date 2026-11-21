# image

Kernel config and unified kernel image build scripts for the paralanes
appliance: UEFI boots a minimal Linux kernel whose only userspace is
`paralanes-init` as PID 1. Linux only. Built in M10 by agent A.

Gate: boots in QEMU and runs `diagsteno`.
