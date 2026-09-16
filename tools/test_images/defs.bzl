"""Bazel-owned Docker archives and runfiles for integration tests."""

load("@rules_oci//oci:defs.bzl", "oci_load")

def test_image(name, image, repository, tag_suffix = ""):
    """Produce an archive tagged with the digest exported by an oci.pull."""
    native.genrule(
        name = name + "_ref",
        srcs = [image + "//:digest"],
        outs = [name + ".ref.txt"],
        cmd = "digest=$$(cat $(SRCS)); printf '%s\\n' '" + repository + "'\"$${digest/:/-}\"'" + tag_suffix + "' > $@",
    )
    oci_load(
        name = name + "_load",
        image = image,
        repo_tags = ":" + name + "_ref",
    )
    native.filegroup(
        name = name + "_tar",
        srcs = [":" + name + "_load"],
        output_group = "tarball",
    )

def image_data(name):
    """Explicit archive and reference inputs for a test."""
    return ["//tools/test_images:" + name + suffix for suffix in ["_tar", "_ref"]]

def image_env(name, prefix = "POSTGRES"):
    """Canonical runfile paths, resolved by each language's runfiles library."""
    return {
        prefix + "_IMAGE_TAR": "$(rlocationpath //tools/test_images:" + name + "_tar)",
        prefix + "_IMAGE_REF": "$(rlocationpath //tools/test_images:" + name + "_ref)",
    }
