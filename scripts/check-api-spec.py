#!/usr/bin/env python3
"""
Coordinator API OpenAPI conformance test.

Two modes:
  1. Offline (default / CI without a live coordinator):
     Validates the spec file itself for structural correctness using
     the schemathesis schema-loader.

  2. Live (requires a running coordinator):
     Set COORDINATOR_URL=http://localhost:8080 and the script will
     run schemathesis against the live server, checking:
       - not_a_server_error   (no 5xx from well-formed requests)
       - response_schema_conformance  (response bodies match the spec)
       - status_code_conformance      (only documented status codes returned)

Usage
-----
# Offline spec validation only (used in CI on every push):
  python3 scripts/check-api-spec.py

# Live conformance test against a running coordinator (used in e2e CI):
  COORDINATOR_URL=http://localhost:8080 python3 scripts/check-api-spec.py

Exit codes
----------
  0  all checks passed
  1  one or more checks failed
"""

import os
import sys
import pathlib
import subprocess

SPEC_PATH = pathlib.Path(__file__).parent.parent / "docs" / "openapi.yaml"
COORDINATOR_URL = os.environ.get("COORDINATOR_URL", "")

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def load_spec():
    """Parse and return the raw OpenAPI YAML document as a dict."""
    try:
        import yaml  # PyYAML
    except ImportError:
        print("ERROR: PyYAML not installed. Run: pip install pyyaml", file=sys.stderr)
        sys.exit(1)

    with open(SPEC_PATH) as f:
        return yaml.safe_load(f)


def validate_spec_structure(spec: dict) -> bool:
    """
    Minimal structural validation so we catch obvious spec regressions.
    Returns True if spec is valid, False otherwise.
    """
    ok = True
    errors = []

    # Top-level required keys
    for key in ("openapi", "info", "paths"):
        if key not in spec:
            errors.append(f"Missing top-level key: '{key}'")
            ok = False

    # Every path must have at least one HTTP method with a 'responses' block
    for path, path_item in spec.get("paths", {}).items():
        for method in ("get", "post", "put", "patch", "delete"):
            op = path_item.get(method)
            if op is None:
                continue
            if "responses" not in op:
                errors.append(f"  {method.upper()} {path}: missing 'responses'")
                ok = False
            if "operationId" not in op:
                errors.append(f"  {method.upper()} {path}: missing 'operationId' (recommended)")
                # not a hard failure

    # Check all $ref targets exist inside the same document
    import json
    spec_text = json.dumps(spec)
    components = spec.get("components", {}).get("schemas", {})
    params    = spec.get("components", {}).get("parameters", {})

    import re
    for ref in re.findall(r'"#/components/schemas/([^"]+)"', spec_text):
        if ref not in components:
            errors.append(f"  Broken $ref: #/components/schemas/{ref}")
            ok = False
    for ref in re.findall(r'"#/components/parameters/([^"]+)"', spec_text):
        if ref not in params:
            errors.append(f"  Broken $ref: #/components/parameters/{ref}")
            ok = False

    # Print findings
    if errors:
        for e in errors:
            print(f"[SPEC ERROR] {e}", file=sys.stderr)
    else:
        print("[OK] Spec structure is valid.")

    return ok


def run_schemathesis_offline() -> bool:
    """
    Use the schemathesis Python API to load the spec and confirm it parses
    without errors (exercising its own validation logic).
    """
    try:
        import schemathesis
    except ImportError:
        print(
            "WARNING: schemathesis not installed; skipping schema-loader check.\n"
            "         Install with: pip install schemathesis",
            file=sys.stderr,
        )
        return True  # non-blocking in offline mode

    loader_candidates = [
        ("schemathesis.from_path", getattr(schemathesis, "from_path", None)),
        (
            "schemathesis.openapi.from_path",
            getattr(getattr(schemathesis, "openapi", None), "from_path", None),
        ),
        (
            "schemathesis.openapi.from_file",
            getattr(getattr(schemathesis, "openapi", None), "from_file", None),
        ),
    ]

    loader_errors = []
    for loader_name, loader in loader_candidates:
        if not callable(loader):
            continue
        try:
            if loader_name.endswith("from_file"):
                with open(SPEC_PATH, "rb") as fd:
                    schema = loader(fd)
            else:
                schema = loader(str(SPEC_PATH))

            count = None
            if hasattr(schema, "get_all_operations"):
                count = sum(1 for _ in schema.get_all_operations())
            elif hasattr(schema, "get_all_endpoints"):
                count = sum(1 for _ in schema.get_all_endpoints())
            elif hasattr(schema, "__iter__"):
                count = sum(1 for _ in schema)

            if count is None:
                print(f"[OK] schemathesis loaded spec via {loader_name}.")
            else:
                print(f"[OK] schemathesis loaded spec via {loader_name}: {count} operations found.")
            return True
        except Exception as exc:
            loader_errors.append(f"{loader_name}: {exc}")

    # Fall back to the CLI if the Python API shape changed again. The CLI is
    # generally more stable across releases than the import surface.
    cli_cmd = [sys.executable, "-m", "schemathesis", "run", str(SPEC_PATH), "--dry-run"]
    result = subprocess.run(cli_cmd, capture_output=True, text=True)
    if result.returncode == 0:
        print("[OK] schemathesis loaded spec via CLI dry-run.")
        return True

    details = "; ".join(loader_errors) if loader_errors else "no compatible schemathesis loader found"
    cli_output = (result.stderr or result.stdout).strip()
    if cli_output:
        details = f"{details}; CLI dry-run: {cli_output}"
    print(f"[FAIL] schemathesis could not load spec: {details}", file=sys.stderr)
    return False


def run_schemathesis_live() -> bool:
    """
    Run a real schemathesis CLI conformance test against a live coordinator.
    Uses subprocess so the output streams directly to stdout/stderr.
    """
    checks = ",".join([
        "not_a_server_error",
        "response_schema_conformance",
        "status_code_conformance",
    ])

    # Endpoints that require auth or a running MPC stack are filtered out so
    # offline/CI runs against the coordinator in isolation still succeed.
    skip_operations = ",".join([
        "requestDeal",
        "requestReveal",
        "requestShowdown",
        "cancelMpcSession",
        "getPlayerCards",
        "playerAction",
        "createTable",
        "joinTable",
    ])

    cmd = [
        sys.executable, "-m", "schemathesis", "run",
        str(SPEC_PATH),
        "--base-url", COORDINATOR_URL,
        "--checks", checks,
        "--exclude-operations", skip_operations,
        "--validate-schema", "true",
        "--hypothesis-max-examples", "10",
        "--exitfirst",
    ]

    print(f"[INFO] Running: {' '.join(cmd)}")
    result = subprocess.run(cmd)
    return result.returncode == 0


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> int:
    if not SPEC_PATH.exists():
        print(f"ERROR: OpenAPI spec not found at {SPEC_PATH}", file=sys.stderr)
        return 1

    print(f"Coordinator API conformance check")
    print(f"  Spec : {SPEC_PATH}")
    print(f"  Mode : {'live — ' + COORDINATOR_URL if COORDINATOR_URL else 'offline'}")
    print()

    spec = load_spec()
    ok = validate_spec_structure(spec)

    ok = run_schemathesis_offline() and ok

    if COORDINATOR_URL:
        ok = run_schemathesis_live() and ok

    print()
    if ok:
        print("PASS: All conformance checks passed.")
        return 0
    else:
        print("FAIL: One or more conformance checks failed.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
