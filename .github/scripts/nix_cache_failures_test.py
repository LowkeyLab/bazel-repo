"""Publication fault evidence must demonstrate a real timeout."""

import pytest
from nix_cache_failures import require_publication_timeout


@pytest.mark.parametrize("status", [1, 124, 137])
def test_immediate_client_exit_does_not_prove_timeout(tmp_path, status):
    status_file = tmp_path / "publication-client.exit-status"
    status_file.write_text(f"{status}\n")

    with pytest.raises(ValueError, match="must reach its 10-second timeout"):
        require_publication_timeout(status_file, 0.05)


@pytest.mark.parametrize(("status", "elapsed"), [(124, 10.01), (137, 15.009)])
def test_actual_timeout_and_kill_evidence_are_accepted(tmp_path, status, elapsed):
    status_file = tmp_path / "publication-client.exit-status"
    status_file.write_text(f"{status}\n")

    require_publication_timeout(status_file, elapsed)
