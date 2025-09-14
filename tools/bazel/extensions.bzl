"""
Module extensions for the lowkeylab project.

This module defines a `gh_releases` extension that allows downloading
artifacts from GitHub releases and making them available as Bazel external
repositories.
"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")
load("//tools/bazel:rules.bzl", "gh_releases_repo")

# Define the attributes that modules can specify.
gh_release_tag = tag_class(attrs = {
    "url": attr.string(mandatory = True),
    "sha256": attr.string(mandatory = True),
    "strip_prefix": attr.string(),
})

def _gh_release_ext_impl(module_ctx):
    """Implementation of the gh_release module extension."""
    releases = {}

    # Iterate over all modules that use this extension.
    for mod in module_ctx.modules:
        # Each 'mod' is a module that uses a gh_release.download tag.
        for tag in mod.tags.download:
            # Generate a unique, internal repository name for the download.
            # Using the module name ensures no collisions.
            repo_name = "_gh_download_{}".format(mod.name)

            http_archive(
                name = repo_name,
                url = tag.url,
                sha256 = tag.sha256,
                strip_prefix = tag.strip_prefix if tag.strip_prefix else "",
            )
            releases[mod.name] = "@" + repo_name

    # Call the repository rule once to create the final @gh_releases repo.
    gh_releases_repo(
        name = "gh_releases",
        releases = releases,
    )

gh_release_ext = module_extension(
    implementation = _gh_release_ext_impl,
    tag_classes = {"download": gh_release_tag},
)
