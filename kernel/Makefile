# SPDX-License-Identifier: GPL-2.0-only
# Convenience wrapper: `make KDIR=/lib/modules/$(uname -r)/build`
KDIR ?= /lib/modules/$$(uname -r)/build
.PHONY: all clean load unload
all:
	$(MAKE) -C $(KDIR) M=$$PWD $(MFLAGS) modules
clean:
	$(MAKE) -C $(KDIR) M=$$PWD $(MFLAGS) clean
load: all
	sudo insmod tb_rdma.ko
unload:
	sudo rmmod tb_rdma
