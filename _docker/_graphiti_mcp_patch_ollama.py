#!/usr/bin/env python3
"""Patch zepai/knowledge-graph-mcp for local Ollama on crack-docker-net.

1. FastMCP DNS-rebinding: constructed with default host=127.0.0.1 before Graphiti
   sets 0.0.0.0, so Host: graphiti-mcp:8000 is rejected with HTTP 421.
2. LLM factory: uses OpenAIClient (/v1/responses). Ollama only speaks
   /v1/chat/completions — switch to OpenAIGenericClient and pass api_url as base_url.
3. qwen3.5 thinking: inject reasoning_effort=none into every chat.completions
   call (and cap max_tokens) so Graphiti extracts don't burn the GPU for minutes.
"""
from __future__ import annotations

from pathlib import Path

FASTMCP = Path(
    "/app/mcp/.venv/lib/python3.11/site-packages/mcp/server/fastmcp/server.py"
)
FACTORIES = Path("/app/mcp/src/services/factories.py")
GENERIC_CLIENT = Path(
    "/app/mcp/.venv/lib/python3.11/site-packages/"
    "graphiti_core/llm_client/openai_generic_client.py"
)
RERANKER = Path(
    "/app/mcp/.venv/lib/python3.11/site-packages/"
    "graphiti_core/cross_encoder/openai_reranker_client.py"
)


def patch_fastmcp() -> None:
    text = FASTMCP.read_text(encoding="utf-8")
    old = "enable_dns_rebinding_protection=True"
    new = "enable_dns_rebinding_protection=False"
    if old in text:
        FASTMCP.write_text(text.replace(old, new), encoding="utf-8")
        print(f"patched {FASTMCP}: disabled DNS rebinding protection")
    else:
        print(f"skip {FASTMCP}: already patched or marker missing")


def patch_factories() -> None:
    text = FACTORIES.read_text(encoding="utf-8")
    if "OpenAIGenericClient" in text and "base_url=config.providers.openai.api_url" in text:
        print(f"skip {FACTORIES}: already patched for Ollama")
        return

    text2 = text.replace(
        "from graphiti_core.llm_client import LLMClient, OpenAIClient",
        "from graphiti_core.llm_client import LLMClient, OpenAIClient\n"
        "from graphiti_core.llm_client.openai_generic_client import OpenAIGenericClient",
    )
    if text2 == text:
        raise SystemExit(f"failed to add OpenAIGenericClient import in {FACTORIES}")

    old_block = """                llm_config = CoreLLMConfig(
                    api_key=api_key,
                    model=config.model,
                    small_model=small_model,
                    temperature=config.temperature,
                    max_tokens=config.max_tokens,
                )

                # Check if this is a reasoning model (o1, o3, gpt-5 family)
                reasoning_prefixes = ('o1', 'o3', 'gpt-5')
                is_reasoning_model = config.model.startswith(reasoning_prefixes)

                # Only pass reasoning/verbosity parameters for reasoning models (gpt-5 family)
                if is_reasoning_model:
                    return OpenAIClient(config=llm_config, reasoning='minimal', verbosity='low')
                else:
                    # For non-reasoning models, explicitly pass None to disable these parameters
                    return OpenAIClient(config=llm_config, reasoning=None, verbosity=None)
"""
    new_block = """                # Ollama / OpenAI-compatible local servers need chat.completions
                # (OpenAIGenericClient). OpenAIClient hits /v1/responses.
                # Cap tokens: qwen3.5 thinking can otherwise fill a 16k budget.
                max_tokens = min(int(config.max_tokens or 2048), 2048)
                llm_config = CoreLLMConfig(
                    api_key=api_key,
                    model=config.model,
                    small_model=small_model,
                    temperature=config.temperature,
                    max_tokens=max_tokens,
                    base_url=config.providers.openai.api_url,
                )
                return OpenAIGenericClient(config=llm_config, max_tokens=max_tokens)
"""
    if old_block not in text2:
        raise SystemExit(f"failed to locate OpenAIClient return block in {FACTORIES}")
    text2 = text2.replace(old_block, new_block, 1)
    FACTORIES.write_text(text2, encoding="utf-8")
    print(f"patched {FACTORIES}: OpenAIGenericClient + base_url for Ollama")


def _inject_extra_body(block: str) -> str:
    """Insert extra_body=... before the closing paren of a create( call block."""
    marker = "extra_body={'reasoning_effort': 'none'}"
    if marker in block:
        return block
    # Insert before the final `)` of the block (last non-empty line).
    lines = block.splitlines(keepends=True)
    for i in range(len(lines) - 1, -1, -1):
        if lines[i].strip() == ")":
            indent = lines[i][: len(lines[i]) - len(lines[i].lstrip())]
            # Match indent of previous kwarg line if possible.
            if i > 0:
                indent = lines[i - 1][: len(lines[i - 1]) - len(lines[i - 1].lstrip())]
            lines.insert(i, f"{indent}{marker},  # disable qwen3.5 thinking\n")
            return "".join(lines)
    raise ValueError("no closing paren in block")


def patch_generic_client_no_think() -> None:
    """Inject reasoning_effort=none into OpenAIGenericClient chat.completions."""
    text = GENERIC_CLIENT.read_text(encoding="utf-8")
    if "reasoning_effort" in text:
        print(f"skip {GENERIC_CLIENT}: already has reasoning_effort")
        return
    # Older knowledge-graph-mcp image uses self.max_tokens + response_format var.
    # Newer graphiti-core uses max_tokens arg + _build_response_format().
    needles = [
        "response_format=response_format,  # type: ignore[arg-type]\n            )",
        "response_format=self._build_response_format(response_model),  # type: ignore[arg-type]\n            )",
    ]
    for needle in needles:
        if needle in text:
            replacement = needle.replace(
                "\n            )",
                "\n                extra_body={'reasoning_effort': 'none'},  # disable qwen3.5 thinking\n            )",
                1,
            )
            GENERIC_CLIENT.write_text(text.replace(needle, replacement, 1), encoding="utf-8")
            print(f"patched {GENERIC_CLIENT}: reasoning_effort=none")
            return
    raise SystemExit(f"failed to locate chat.completions.create in {GENERIC_CLIENT}")


def patch_reranker_no_think() -> None:
    """Same for the boolean reranker (also hits chat.completions)."""
    if not RERANKER.exists():
        print(f"skip {RERANKER}: missing")
        return
    text = RERANKER.read_text(encoding="utf-8")
    if "reasoning_effort" in text:
        print(f"skip {RERANKER}: already has reasoning_effort")
        return
    needle = "top_logprobs=2,\n                    )"
    if needle not in text:
        print(f"skip {RERANKER}: create() block not found (version skew)")
        return
    replacement = (
        "top_logprobs=2,\n"
        "                        extra_body={'reasoning_effort': 'none'},\n"
        "                    )"
    )
    RERANKER.write_text(text.replace(needle, replacement, 1), encoding="utf-8")
    print(f"patched {RERANKER}: reasoning_effort=none")


if __name__ == "__main__":
    try:
        patch_fastmcp()
        patch_factories()
        patch_generic_client_no_think()
        patch_reranker_no_think()
    except SystemExit as exc:
        # Don't crash-loop the container on a version-skewed patch; log and boot.
        print(f"WARNING: ollama patch incomplete: {exc}", flush=True)
