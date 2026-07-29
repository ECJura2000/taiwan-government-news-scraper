import importlib.util
from pathlib import Path
import sys


def load_audit_module():
    path = Path(__file__).resolve().parents[1] / "scripts" / "audit_ssl_allowlist.py"
    spec = importlib.util.spec_from_file_location("audit_ssl_allowlist", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_approved_removals_require_success_in_both_environments():
    audit = load_audit_module()
    local = {
        "results": [
            {"host": "safe.example", "success": True},
            {"host": "local-only.example", "success": True},
        ]
    }
    github = {
        "results": [
            {"host": "safe.example", "success": True},
            {"host": "local-only.example", "success": False},
        ]
    }

    assert audit.approved_intersection(local, github) == ["safe.example"]


def test_failed_probe_keeps_error_evidence(monkeypatch):
    audit = load_audit_module()

    def fail(*_args, **_kwargs):
        raise audit.requests.exceptions.SSLError("certificate failed")

    monkeypatch.setattr(audit.requests, "get", fail)

    result = audit.probe_host("unsafe.example", timeout=1)

    assert result["success"] is False
    assert result["status_code"] is None
    assert "certificate failed" in result["error"]
