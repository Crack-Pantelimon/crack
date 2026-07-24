#!/usr/bin/env python3
"""Patch zepai/knowledge-graph-mcp for local Ollama on crack-docker-net.

1. FastMCP DNS-rebinding: constructed with default host=127.0.0.1 before Graphiti
   sets 0.0.0.0, so Host: graphiti-mcp:8000 is rejected with HTTP 421.
2. LLM factory: uses OpenAIClient (/v1/responses). Ollama only speaks
   /v1/chat/completions — switch to OpenAIGenericClient and pass api_url as base_url.
3. qwen3.5 thinking: inject reasoning_effort=none into every chat.completions
   call (and cap max_tokens) so Graphiti extracts don't burn the GPU for minutes.
4. Queue worker: keep strong refs to asyncio tasks (fire-and-forget create_task can
   be GC'd before the worker starts — episodes stay "queued" forever).
5. add_memory: when GRAPHITI_SYNC_ADD=1 (default), await episode processing so MCP
   callers don't report success while the FalkorDB graph is still empty.
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
QUEUE_SERVICE = Path("/app/mcp/src/services/queue_service.py")
MCP_SERVER = Path("/app/mcp/src/graphiti_mcp_server.py")


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


def patch_queue_strong_refs_and_done_event() -> None:
    """Keep worker task refs + signal completion via asyncio.Event on each episode."""
    text = QUEUE_SERVICE.read_text(encoding="utf-8")
    if "_worker_tasks" in text and "done_event" in text:
        print(f"skip {QUEUE_SERVICE}: already patched for strong refs / done_event")
        return

    old_init = '''    def __init__(self):
        """Initialize the queue service."""
        # Dictionary to store queues for each group_id
        self._episode_queues: dict[str, asyncio.Queue] = {}
        # Dictionary to track if a worker is running for each group_id
        self._queue_workers: dict[str, bool] = {}
        # Store the graphiti client after initialization
        self._graphiti_client: Any = None
'''
    new_init = '''    def __init__(self):
        """Initialize the queue service."""
        # Dictionary to store queues for each group_id
        self._episode_queues: dict[str, asyncio.Queue] = {}
        # Dictionary to track if a worker is running for each group_id
        self._queue_workers: dict[str, bool] = {}
        # Strong refs so fire-and-forget create_task workers are not GC'd
        self._worker_tasks: dict[str, asyncio.Task] = {}
        # Store the graphiti client after initialization
        self._graphiti_client: Any = None
'''
    if old_init not in text:
        raise SystemExit(f"failed to locate QueueService.__init__ in {QUEUE_SERVICE}")
    text = text.replace(old_init, new_init, 1)

    old_start = '''        # Start a worker for this queue if one isn't already running
        if not self._queue_workers.get(group_id, False):
            asyncio.create_task(self._process_episode_queue(group_id))

        return self._episode_queues[group_id].qsize()
'''
    new_start = '''        # Start a worker for this queue if one isn't already running.
        # Keep a strong reference — bare create_task can be garbage-collected
        # before the worker starts, leaving episodes stuck "queued" forever.
        if not self._queue_workers.get(group_id, False):
            self._worker_tasks[group_id] = asyncio.create_task(
                self._process_episode_queue(group_id)
            )

        return self._episode_queues[group_id].qsize()
'''
    if old_start not in text:
        raise SystemExit(f"failed to locate create_task site in {QUEUE_SERVICE}")
    text = text.replace(old_start, new_start, 1)

    old_process = '''        async def process_episode():
            """Process the episode using the graphiti client."""
            try:
                logger.info(f'Processing episode {uuid} for group {group_id}')

                # Process the episode using the graphiti client
                await self._graphiti_client.add_episode(
                    name=name,
                    episode_body=content,
                    source_description=source_description,
                    source=episode_type,
                    group_id=group_id,
                    reference_time=datetime.now(timezone.utc),
                    entity_types=entity_types,
                    uuid=uuid,
                )

                logger.info(f'Successfully processed episode {uuid} for group {group_id}')

            except Exception as e:
                logger.error(f'Failed to process episode {uuid} for group {group_id}: {str(e)}')
                raise

        # Use the existing add_episode_task method to queue the processing
        return await self.add_episode_task(group_id, process_episode)
'''
    new_process = '''        done_event = asyncio.Event()
        error_box: list[BaseException] = []

        async def process_episode():
            """Process the episode using the graphiti client."""
            try:
                logger.info(f'Processing episode {uuid} for group {group_id}')

                # Process the episode using the graphiti client
                await self._graphiti_client.add_episode(
                    name=name,
                    episode_body=content,
                    source_description=source_description,
                    source=episode_type,
                    group_id=group_id,
                    reference_time=datetime.now(timezone.utc),
                    entity_types=entity_types,
                    uuid=uuid,
                )

                logger.info(f'Successfully processed episode {uuid} for group {group_id}')

            except Exception as e:
                logger.error(f'Failed to process episode {uuid} for group {group_id}: {str(e)}')
                error_box.append(e)
                raise
            finally:
                done_event.set()

        # Use the existing add_episode_task method to queue the processing
        position = await self.add_episode_task(group_id, process_episode)

        # Optional sync wait (GRAPHITI_SYNC_ADD=1): MCP returns only after nodes land.
        if os.environ.get('GRAPHITI_SYNC_ADD', '1') == '1':
            await done_event.wait()
            if error_box:
                raise error_box[0]

        return position
'''
    if old_process not in text:
        raise SystemExit(f"failed to locate process_episode in {QUEUE_SERVICE}")
    if "import os" not in text:
        text = text.replace(
            "import asyncio\nimport logging\n",
            "import asyncio\nimport logging\nimport os\n",
            1,
        )
    text = text.replace(old_process, new_process, 1)
    QUEUE_SERVICE.write_text(text, encoding="utf-8")
    print(f"patched {QUEUE_SERVICE}: strong worker refs + optional sync wait")


def patch_add_memory_message() -> None:
    """Clarify SuccessResponse when GRAPHITI_SYNC_ADD waits for completion."""
    text = MCP_SERVER.read_text(encoding="utf-8")
    if "processed into group" in text:
        print(f"skip {MCP_SERVER}: add_memory message already patched")
        return
    old = '''        return SuccessResponse(
            message=f"Episode '{name}' queued for processing in group '{effective_group_id}'"
        )
'''
    new = '''        sync = os.environ.get('GRAPHITI_SYNC_ADD', '1') == '1'
        verb = 'processed into' if sync else 'queued for processing in'
        return SuccessResponse(
            message=f"Episode '{name}' {verb} group '{effective_group_id}'"
        )
'''
    if old not in text:
        raise SystemExit(f"failed to locate SuccessResponse in {MCP_SERVER}")
    text = text.replace(old, new, 1)
    MCP_SERVER.write_text(text, encoding="utf-8")
    print(f"patched {MCP_SERVER}: sync-aware add_memory message")


if __name__ == "__main__":
    try:
        patch_fastmcp()
        patch_factories()
        patch_generic_client_no_think()
        patch_reranker_no_think()
        patch_queue_strong_refs_and_done_event()
        patch_add_memory_message()
    except SystemExit as exc:
        # Don't crash-loop the container on a version-skewed patch; log and boot.
        print(f"WARNING: ollama patch incomplete: {exc}", flush=True)
