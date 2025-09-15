"""Bazel extension for downloading ZIP files from GitHub releases."""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_file")

def _gh_release_zip_impl(module_ctx):
    """Implementation function for the gh_release_zip extension."""

    for mod in module_ctx.modules:
        for tag in mod.tags.download:
            # Construct the download URL
            url = "https://github.com/{owner}/{repo}/archive/refs/tags/{tag}.zip".format(
                owner = tag.owner,
                repo = tag.repo,
                tag = tag.tag,
            )

            # Create the repository using http_file
            http_file(
                name = tag.name,
                url = url,
                sha256 = tag.sha256 if hasattr(tag, "sha256") and tag.sha256 else "",
                downloaded_file_path = "{repo}-{tag}.zip".format(
                    repo = tag.repo,
                    tag = tag.tag.lstrip("v"),  # Remove 'v' prefix if present
                ),
            )

# Define the tag class for download configuration
_gh_release_download_tag = tag_class(
    attrs = {
        "name": attr.string(
            doc = "Name of the repository to create",
            mandatory = True,
        ),
        "owner": attr.string(
            doc = "GitHub repository owner/organization",
            mandatory = True,
        ),
        "repo": attr.string(
            doc = "GitHub repository name",
            mandatory = True,
        ),
        "tag": attr.string(
            doc = "Git tag/release version to download",
            mandatory = True,
        ),
        "sha256": attr.string(
            doc = "Expected SHA256 hash of the downloaded archive (optional but recommended)",
            mandatory = False,
        ),
    },
)

# Define the extension
gh_release_zip = module_extension(
    implementation = _gh_release_zip_impl,
    tag_classes = {
        "download": _gh_release_download_tag,
    },
)
