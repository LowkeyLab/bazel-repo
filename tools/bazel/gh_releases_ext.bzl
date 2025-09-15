"""Bazel extension for downloading ZIP files from GitHub releases."""

def _gh_release_file_repository_impl(rctx):
    """Implementation for a repository rule that downloads a file and creates a BUILD file."""

    # Download the file
    rctx.download(
        url = rctx.attr.url,
        sha256 = rctx.attr.sha256,
        output = rctx.attr.filename,
    )

    # Create BUILD file
    rctx.file("BUILD.bazel", """
filegroup(
    name = "file",
    srcs = ["{filename}"],
    visibility = ["//visibility:public"],
)
""".format(filename = rctx.attr.filename))

# Define the repository rule
_gh_release_file_repository = repository_rule(
    implementation = _gh_release_file_repository_impl,
    attrs = {
        "url": attr.string(mandatory = True),
        "sha256": attr.string(mandatory = False, default = ""),
        "filename": attr.string(mandatory = True),
    },
)

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

            # Construct filename
            filename = "{repo}-{tag}.zip".format(
                repo = tag.repo,
                tag = tag.tag.lstrip("v"),  # Remove 'v' prefix if present
            )

            # Create the repository using our custom rule
            _gh_release_file_repository(
                name = tag.name,
                url = url,
                sha256 = tag.sha256 if hasattr(tag, "sha256") and tag.sha256 else "",
                filename = filename,
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
