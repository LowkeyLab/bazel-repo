"""Deterministic strict-download provenance policy; callers supply observations."""

import json


def _downloaded_sources(stderr, allowed):
    sources = {}
    for line in stderr.splitlines():
        if not line.startswith("@nix "):
            continue
        event = json.loads(line[5:])
        if not isinstance(event, dict):
            # Invalid captured evidence follows the normal policy-failure path.
            raise ValueError("Nix activity record must be an object")  # noqa: TRY004
        if event.get("action") != "start":
            continue
        if event.get("type") == 105:
            raise ValueError("Nix reported a build activity during strict acceptance")
        if event.get("type") != 108:
            continue
        fields = event.get("fields")
        if not isinstance(fields, list) or len(fields) != 2:
            raise ValueError("Unknown Nix substitution log schema")
        path, source = fields
        if (
            not isinstance(path, str)
            or not isinstance(source, str)
            or source not in allowed
        ):
            raise ValueError("Unknown substitution path or source in Nix log")
        sources.setdefault(path, set()).add(source)
    return sources


class ValidatedDownloads:
    """Root-validated activity evidence used to classify the runtime closure."""

    def __init__(self, tool, output, url, activity_log):
        sources = _downloaded_sources(activity_log, {url, "https://cache.nixos.org"})
        if output not in sources or len(sources[output]) != 1:
            raise ValueError("Output download provenance is absent or ambiguous")
        if tool == "aspect" and sources[output] != {url}:
            raise ValueError(
                "Aspect root was not downloaded exclusively from intended Cachix"
            )
        self._sources = sources

    def classify(self, initial, closure):
        """Return ordered path records, retaining preexisting-path precedence."""
        records = []
        for path in closure:
            if path in initial:
                records.append({"path": path, "status": "present-before-realisation"})
            elif path not in self._sources or len(self._sources[path]) != 1:
                raise ValueError(
                    f"Closure download provenance absent or ambiguous: {path}"
                )
            else:
                records.append(
                    {
                        "path": path,
                        "source": next(iter(self._sources[path])),
                        "status": "downloaded",
                    }
                )
        return records
