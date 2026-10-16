"""Public cache configuration behavior for credential-free CI readers."""

import os

import pytest
from nix_tool import configure


@pytest.mark.parametrize("empty", [None, ""])
def test_fork_reader_uses_verified_public_cache_without_variables(monkeypatch, empty):
    for name in ("CACHIX_CACHE_NAME", "CACHIX_PUBLIC_KEY"):
        if empty is None:
            monkeypatch.delenv(name, raising=False)
        else:
            monkeypatch.setenv(name, empty)
    monkeypatch.setenv("NIX_CONFIG", "")

    assert configure() == "https://lowkey-lab.cachix.org"
    assert (
        "extra-substituters = https://lowkey-lab.cachix.org" in os.environ["NIX_CONFIG"]
    )
    assert (
        "extra-trusted-public-keys = lowkey-lab.cachix.org-1:7YzknrrOt9NRhJp4VIYCqa9Edc/401MuFrtqyPUW3i8="
        in os.environ["NIX_CONFIG"]
    )
    assert "require-sigs = true" in os.environ["NIX_CONFIG"]


def test_explicit_complete_public_cache_override_is_preserved(monkeypatch):
    monkeypatch.setenv("CACHIX_CACHE_NAME", "override-cache")
    monkeypatch.setenv(
        "CACHIX_PUBLIC_KEY",
        "override-cache.cachix.org-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    )
    monkeypatch.setenv("NIX_CONFIG", "connect-timeout = 9")

    assert configure() == "https://override-cache.cachix.org"
    assert (
        "extra-substituters = https://override-cache.cachix.org"
        in os.environ["NIX_CONFIG"]
    )
    assert (
        "extra-trusted-public-keys = override-cache.cachix.org-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
        in os.environ["NIX_CONFIG"]
    )
    assert "https://lowkey-lab.cachix.org" not in os.environ["NIX_CONFIG"]
    assert os.environ["NIX_CONFIG"].startswith("connect-timeout = 9")


@pytest.mark.parametrize(
    ("name", "key"),
    [
        ("override-cache", ""),
        (
            "",
            "override-cache.cachix.org-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        ),
        ("override-cache", "invalid-key"),
    ],
)
def test_partial_or_invalid_public_override_still_fails(monkeypatch, name, key):
    monkeypatch.setenv("CACHIX_CACHE_NAME", name)
    monkeypatch.setenv("CACHIX_PUBLIC_KEY", key)
    monkeypatch.setenv("NIX_CONFIG", "connect-timeout = 9")

    with pytest.raises(ValueError, match="Set both CACHIX_CACHE_NAME"):
        configure()
    assert os.environ["NIX_CONFIG"] == "connect-timeout = 9"
