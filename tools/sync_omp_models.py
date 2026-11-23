#!/usr/bin/env python3
"""Translate ~/.omp/agent/models.yml into a dsh pi-ai profile route set.

OMP keeps provider JSON in one personal file. dsh keeps routes in the booth
profile's cordis.patch.yml and credentials in $DSH_HOME/.env. This script maps
the OMP providers to dsh `llm-pi-ai` entries, discovers models for entries whose
OMP `models` list is empty, and writes API keys only to .env (never to the
profile). Existing patch rows are preserved.

Usage:
  python3 tools/sync_omp_models.py [--models PATH] [--dsh-home PATH] [--dry-run]
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.request
from pathlib import Path

import yaml

PROTOCOLS = {"openai-responses", "openai-completions", "anthropic-messages"}


def fetch_models(base_url: str, api: str, key: str) -> list[dict]:
    url = base_url.rstrip("/") + "/models"
    headers = {"User-Agent": "curl/8"} if api != "anthropic-messages" else {
        "User-Agent": "curl/8", "x-api-key": key, "anthropic-version": "2023-06-01"}
    if api != "anthropic-messages": headers["Authorization"] = f"Bearer {key}"
    request = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            payload = json.load(response)
    except Exception as error:  # noqa: BLE001 - report and keep the route unconfigured
        print(f"warning: cannot discover {base_url}: {error}", file=sys.stderr)
        return []
    rows = payload.get("data", []) if isinstance(payload, dict) else []
    out = []
    for row in rows:
        if isinstance(row, dict) and isinstance(row.get("id"), str):
            out.append({"id": row["id"], "name": row.get("display_name", row["id"])})
    return out



def fetch_opencode_models(key: str) -> list[dict]:
    """Fetch OpenCode Go's full catalog, preserving capacity, modalities and effort levels."""
    request = urllib.request.Request("https://opencode.ai/inference/go/v1/models",
                                     headers={"Authorization": f"Bearer {key}", "User-Agent": "curl/8"})
    with urllib.request.urlopen(request, timeout=15) as response:
        payload = json.load(response)
    allowed = {"off", "minimal", "low", "medium", "high", "xhigh", "max"}
    out = []
    for model in payload.get("data", []):
        if not isinstance(model, dict) or not isinstance(model.get("id"), str): continue
        capabilities = model.get("capabilities") if isinstance(model.get("capabilities"), dict) else {}
        if not capabilities.get("tools", {}).get("supported", False): continue
        row: dict = {"id": model["id"], "name": model.get("name", model["id"])}
        limit = model.get("limit") if isinstance(model.get("limit"), dict) else {}
        if isinstance(limit.get("context"), int): row["contextWindow"] = limit["context"]
        if isinstance(limit.get("output"), int): row["maxTokens"] = limit["output"]
        modalities = model.get("modalities") if isinstance(model.get("modalities"), dict) else {}
        inputs = modalities.get("input") if isinstance(modalities.get("input"), list) else []
        kept = ["text"] if "text" in inputs else []
        if any(str(item).startswith("image/") for item in inputs): kept.append("image")
        if kept: row["input"] = kept
        efforts = capabilities.get("reasoning", {}).get("effort")
        if isinstance(efforts, list):
            clean = {level: level for level in efforts if level in allowed}
            if clean: row["reasoningEfforts"] = clean
        out.append(row)
    return out

def effort_map(model: dict) -> dict[str, str] | None:
    thinking = model.get("thinking")
    if not isinstance(thinking, dict):
        return None
    efforts = thinking.get("efforts")
    if isinstance(efforts, list) and efforts:
        return {str(level): str(level) for level in efforts}
    return None


def dsh_model(provider_name: str, model: dict, key: str) -> dict:
    out: dict = {"id": str(model["id"])}
    if model.get("name"):
        out["name"] = str(model["name"])
    if isinstance(model.get("contextWindow"), int):
        out["contextWindow"] = model["contextWindow"]
    if isinstance(model.get("maxTokens"), int):
        out["maxTokens"] = model["maxTokens"]
    inputs = model.get("input")
    if isinstance(inputs, list):
        kept = [item for item in inputs if item in {"text", "image", "audio", "video"}]
        if kept:
            out["input"] = kept
    efforts = effort_map(model)
    if efforts is not None:
        out["reasoningEfforts"] = efforts
    compat = model.get("compat")
    if isinstance(compat, dict) and "supportsDeveloperRole" in compat:
        out["compat"] = {"supportsDeveloperRole": bool(compat["supportsDeveloperRole"])}
    return out


def env_name(provider: str) -> str:
    safe = "".join(ch if ch.isalnum() else "_" for ch in provider).upper()
    return f"OMP_{safe}_API_KEY"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--models", default=str(Path.home() / ".omp" / "agent" / "models.yml"))
    parser.add_argument("--dsh-home", default=os.environ.get("DSH_HOME", str(Path.home() / ".dsh")))
    parser.add_argument("--profile", default=os.environ.get("BOOTH_BASE_PROFILE", "booth"))
    parser.add_argument("--opencode-default", action="store_true",
                        help="also install the OpenCode Go route from OPENCODE_API_KEY and make it the default")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    source = Path(args.models).expanduser()
    if not source.exists():
        print(f"no OMP models file at {source}", file=sys.stderr)
        return 1
    data = yaml.safe_load(source.read_text()) or {}
    providers = data.get("providers") if isinstance(data, dict) else None
    if not isinstance(providers, dict) or not providers:
        print(f"no providers in {source}", file=sys.stderr)
        return 1

    dsh_home = Path(args.dsh_home).expanduser()
    profile_dir = dsh_home / "profiles" / args.profile
    patch_path = profile_dir / "cordis.patch.yml"
    env_path = dsh_home / ".env"

    env_lines = env_path.read_text().splitlines() if env_path.exists() else []
    env_index = {
        line.split("=", 1)[0]: index
        for index, line in enumerate(env_lines)
        if "=" in line and not line.lstrip().startswith("#")
    }

    routes: dict[str, dict] = {}
    for provider_name, provider in providers.items():
        if not isinstance(provider, dict):
            continue
        api = provider.get("api")
        base_url = provider.get("baseUrl")
        key = provider.get("apiKey")
        if api not in PROTOCOLS or not isinstance(base_url, str) or not isinstance(key, str):
            print(f"warning: skipping {provider_name}: unsupported API/endpoint/key", file=sys.stderr)
            continue
        models = provider.get("models")
        model_rows = [m for m in models if isinstance(m, dict)] if isinstance(models, list) else []
        if not model_rows:
            model_rows = fetch_models(base_url, api, key)
        if not model_rows:
            print(f"warning: skipping {provider_name}: no models", file=sys.stderr)
            continue
        route: dict = {
            "displayName": provider_name,
            "apiKeyEnv": env_name(provider_name),
            "api": api,
            "baseURL": base_url,
            "models": [dsh_model(provider_name, model, key) for model in model_rows],
        }
        compat = provider.get("compat")
        if isinstance(compat, dict) and "supportsDeveloperRole" in compat:
            route["compat"] = {"supportsDeveloperRole": bool(compat["supportsDeveloperRole"])}
        routes[provider_name] = route
        name = env_name(provider_name)
        line = f"{name}={key}"
        if name in env_index:
            env_lines[env_index[name]] = line
        else:
            env_index[name] = len(env_lines)
            env_lines.append(line)

    if not routes:
        print("no usable OMP providers", file=sys.stderr)
        return 1

    rows = []
    if patch_path.exists():
        loaded = yaml.safe_load(patch_path.read_text()) or []
        if isinstance(loaded, list):
            rows = loaded
    existing_providers = {}
    for row in rows:
        if isinstance(row, dict) and row.get("id") == "llm-pi-ai":
            config = row.get("config")
            if isinstance(config, dict) and isinstance(config.get("providers"), dict):
                existing_providers = config["providers"]
    merged = {**existing_providers, **routes}
    if args.opencode_default:
        key = os.environ.get("OPENCODE_API_KEY")
        if not key:
            for line in env_lines:
                if line.startswith("OPENCODE_API_KEY="):
                    key = line.split("=", 1)[1]
                    break
        if not key:
            print("warning: --opencode-default needs OPENCODE_API_KEY", file=sys.stderr)
        else:
            models_out = fetch_opencode_models(key)
            merged["opencode"] = {
                "displayName": "opencode", "apiKeyEnv": "OPENCODE_API_KEY",
                "api": "openai-completions", "baseURL": "https://opencode.ai/inference/go/v1",
                "headers": {"x-opencode-session": "booth-dsh"}, "models": models_out,
            }
    replaced = False
    for row in rows:
        if isinstance(row, dict) and row.get("id") == "llm-pi-ai":
            row["config"] = {"providers": merged}
            replaced = True
    if not replaced:
        rows.append({"id": "llm-pi-ai", "config": {"providers": merged}})
    if args.opencode_default and "opencode" in merged:
        rows = [row for row in rows if not (isinstance(row, dict) and row.get("id") == "acp")]
        rows.append({"id": "acp", "config": {"provider": "opencode", "model": "gpt-6-luna"}})
        rows = [row for row in rows if not (isinstance(row, dict) and row.get("id") == "agent-default-model")]
        rows.append({"id": "agent-default-model", "config": {"provider": "opencode", "model": "gpt-6-luna"}})

    patch_text = yaml.safe_dump(rows, sort_keys=False, allow_unicode=True)
    env_text = "\n".join(env_lines) + "\n" if env_lines else ""
    if args.dry_run:
        print(patch_text)
        print(f"# credentials -> {env_path}", file=sys.stderr)
        return 0

    profile_dir.mkdir(parents=True, exist_ok=True)
    patch_path.write_text(patch_text)
    env_path.parent.mkdir(parents=True, exist_ok=True)
    env_path.write_text(env_text)
    os.chmod(env_path, 0o600)
    print(f"wrote {patch_path} with {len(routes)} providers and {env_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
