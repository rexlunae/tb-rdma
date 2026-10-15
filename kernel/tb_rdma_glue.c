// SPDX-License-Identifier: GPL-2.0-only
/*
 * tb_rdma_glue.c - C-side Thunderbolt glue for the tb_rdma Rust module.
 *
 * struct tb_service_driver embeds a struct device_driver whose layout is
 * private to each kernel build, so the registration lives in C and only
 * calls into Rust via the #[no_mangle] functions in tb_rdma_core.rs. This
 * is the out-of-tree stand-in for the tb_rdma_sys bindings described in
 * kernel/README.md ("Order of work" step 2).
 */
#include <linux/module.h>
#include <linux/thunderbolt.h>

int tb_rdma_peer_probe(const void *svc, const char *key, int prtcid,
		       int prtcvers, int prtcrevs, int prtcstns);
void tb_rdma_peer_remove(const void *svc);

static const struct tb_service_id tb_rdma_ids[] = {
	{ TB_SERVICE("tb-rdma", 1) },
	{ },
};
MODULE_DEVICE_TABLE(tbsvc, tb_rdma_ids);

static int tb_rdma_glue_probe(struct tb_service *svc,
			      const struct tb_service_id *id)
{
	/* The id table only matches on key and prtcid; report what the peer advertised. */
	return tb_rdma_peer_probe(svc, svc->key, svc->prtcid, svc->prtcvers,
				  svc->prtcrevs, svc->prtcstns);
}

static void tb_rdma_glue_remove(struct tb_service *svc)
{
	tb_rdma_peer_remove(svc);
}

static struct tb_service_driver tb_rdma_service_driver = {
	.driver = {
		.owner = THIS_MODULE,
		.name = "tb-rdma",
	},
	.probe = tb_rdma_glue_probe,
	.remove = tb_rdma_glue_remove,
	.id_table = tb_rdma_ids,
};

/*
 * Property directory advertised over XDomain discovery. Values mirror what
 * tbnet_init() does for "network": one property per field, so a peer's
 * thunderbolt core lists "tb-rdma" among our services and its service
 * driver can bind to ours.
 */
static struct tb_property_dir *tb_rdma_dir;

int tb_rdma_glue_init(void)
{
	int ret;

	tb_rdma_dir = tb_property_create_dir(NULL);
	if (!tb_rdma_dir)
		return -ENOMEM;

	ret = tb_property_add_immediate(tb_rdma_dir, "prtcid", 1);
	if (!ret)
		ret = tb_property_add_immediate(tb_rdma_dir, "prtcvers", 1);
	if (!ret)
		ret = tb_property_add_immediate(tb_rdma_dir, "prtcrevs", 1);
	if (!ret)
		ret = tb_property_add_immediate(tb_rdma_dir, "prtcstns", 0);
	if (ret)
		goto err_free;

	ret = tb_register_property_dir("tb-rdma", tb_rdma_dir);
	if (ret)
		goto err_free;

	ret = tb_register_service_driver(&tb_rdma_service_driver);
	if (ret)
		goto err_unregister;

	return 0;

err_unregister:
	tb_unregister_property_dir("tb-rdma", tb_rdma_dir);
err_free:
	tb_property_free_dir(tb_rdma_dir);
	tb_rdma_dir = NULL;
	return ret;
}

void tb_rdma_glue_exit(void)
{
	if (tb_rdma_dir) {
		tb_unregister_property_dir("tb-rdma", tb_rdma_dir);
		tb_property_free_dir(tb_rdma_dir);
		tb_rdma_dir = NULL;
	}
	tb_unregister_service_driver(&tb_rdma_service_driver);
}
// License comes from the Rust module! macro ("GPL"); no second
// MODULE_LICENSE here to avoid duplicate modinfo entries.
