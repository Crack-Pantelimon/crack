/**
 * crack — spawn background sub-agents via crack-server personas, block for
 * their results (wait_join), and suspend for human input (ask_user).
 *
 * Tools-only (no slash commands). Personas are read synchronously from
 * .pi/crack/sub_agents/<slug>/config.json at factory time — no HTTP on the
 * registration path. Spawn/wait_join are registered only when
 * CRACK_SUBAGENT_DEPTH < MAX_DEPTH (see sub_agents/constants.py on the server).
 * Chat context (CRACK_CHAT_ID / CRACK_PARENT_* / CRACK_SUBAGENT_DEPTH) is set by
 * the server for sub-agent runs. Rigid pipeline stages stay isolated via their
 * explicit --tools allowlists.
 *
 * Server: http://<host>:9847 (CRACK_PI_HOST, default 127.0.0.1; sandboxes use crack-dev)
 */

import { readdirSync, readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { truncateTail } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

const HOST = process.env.CRACK_PI_HOST ?? "127.0.0.1";
const BASE = `http://${HOST}:${process.env.CRACK_PI_PORT ?? "9847"}`;
// Must match crack_server.sub_agents.constants.MAX_DEPTH
const MAX_DEPTH = 1;
const MAX_PARALLEL_SUBAGENTS = 3;
const TODO_MAX = 12;

const PARAMS = Type.Object({
	instructions: Type.String({ description: "Task for the sub-agent" }),
	plan: Type.Boolean({
		description:
			"REQUIRED. Prewalk plan mode. true: the run starts on a smarter planner " +
			"model that explores and builds a todo list, then hands off to a cheaper " +
			"implementer model the moment it lands its first edit — best for non-trivial " +
			"changes. false: the whole task runs on one model — best for small, mechanical " +
			"edits. Decide deliberately every time; there is no default.",
	}),
});

const TODO_PARAMS = Type.Object({
	action: Type.String({
		description:
			'"write" replaces the whole list with `items`; "toggle" flips item `id` done/undone; "list" re-shows it.',
	}),
	items: Type.Optional(
		Type.Array(Type.String(), {
			description: `Full replacement list of todo texts (action=write). Each becomes an unchecked step. Max ${TODO_MAX}.`,
		}),
	),
	id: Type.Optional(
		Type.Number({ description: "Item number to toggle done/undone (action=toggle)." }),
	),
});

interface Todo {
	id: number;
	text: string;
	done: boolean;
}

interface TodoDetails {
	todos: Todo[];
}

function renderTodos(todos: Todo[]): string {
	if (todos.length === 0) return "Todo list is empty.";
	const done = todos.filter((t) => t.done).length;
	const lines = todos.map((t) => `[${t.done ? "x" : " "}] #${t.id} ${t.text}`);
	return `Todo list (${done}/${todos.length} done):\n${lines.join("\n")}`;
}

const WAIT_PARAMS = Type.Object({
	target: Type.Optional(
		Type.String({
			description:
				"Which child to wait for: a run id, a persona slug (e.g. \"explorer\"), or omit/\"all\" for every outstanding sub-agent.",
		}),
	),
	timeout_seconds: Type.Optional(
		Type.Number({
			description:
				"Max seconds to block (default 600, clamped to 5..3600). Waiting is free — no tokens are burned while blocked. On timeout, call wait_join again or end your turn.",
		}),
	),
});

const ASK_PARAMS = Type.Object({
	question: Type.String({ description: "The question for the human." }),
	choices: Type.Optional(
		Type.Array(Type.String(), {
			description: "Optional multiple-choice options the human picks from.",
		}),
	),
});

const ANALYZE_IMAGE_PARAMS = Type.Object({
	prompt: Type.String({
		description: "What to look for or answer about the image(s).",
	}),
	image_paths: Type.Array(Type.String(), {
		description: "Paths to image files to analyze (must exist and be valid images).",
	}),
});

interface SpawnResult {
	run_id: string;
	report_path: string;
	status?: string;
	waited?: boolean;
}

interface WaitPending {
	run_id: string;
	persona: string;
	phase: string;
	notified: boolean;
}

interface WaitResult {
	run_id: string;
	persona: string;
	status: string;
	text: string;
	delivered_earlier: boolean;
}

interface WaitResponse {
	results: WaitResult[];
	pending: WaitPending[];
}

const FREE_TOOLS = new Set([
	"read",
	"grep",
	"find",
	"ls",
	"todo",
	"wait_join",
	"ask_user",
	"analyze_image",
]);

function isDestructive(toolName: string): boolean {
	if (FREE_TOOLS.has(toolName)) return false;
	if (toolName.startsWith("spawn_")) return false;
	return true; // bash, edit, write, and all mcp/custom tools
}

function clamp(n: number, lo: number, hi: number): number {
	return Math.min(hi, Math.max(lo, n));
}

function sleep(ms: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, ms));
}

function crackContext(): { chatId: string; parentKind: string; parentId: string } {
	const chatId = process.env.CRACK_CHAT_ID;
	if (!chatId) {
		throw new Error(
			"spawn/wait/ask tools only work inside a crack unscripted chat or sub-agent run",
		);
	}
	return {
		chatId,
		parentKind: process.env.CRACK_PARENT_KIND ?? "chat",
		parentId: process.env.CRACK_PARENT_ID ?? chatId,
	};
}

async function hasRunningChildren(): Promise<boolean> {
	const { chatId, parentKind, parentId } = crackContext();
	const url =
		`${BASE}/api/chats/${encodeURIComponent(chatId)}/sub_agents/active_count` +
		`?parent_kind=${encodeURIComponent(parentKind)}` +
		`&parent_id=${encodeURIComponent(parentId)}`;
	const res = await fetch(url, { signal: AbortSignal.timeout(5000) });
	if (!res.ok) {
		throw new Error(truncateTail(await res.text()).content);
	}
	const d = (await res.json()) as { active: number };
	return d.active > 0;
}

/** Block until every running child is terminal (implicit parent-freeze wait). */
async function waitForAllChildren(): Promise<void> {
	const maxTotalSeconds = 3600;
	const deadline = Date.now() + maxTotalSeconds * 1000;
	while (await hasRunningChildren()) {
		const remaining = Math.ceil((deadline - Date.now()) / 1000);
		if (remaining <= 0) {
			throw new Error(
				"Timed out waiting for sub-agents to finish before destructive tool call",
			);
		}
		await executeWaitJoin({ timeout_seconds: Math.min(600, remaining) });
	}
}

async function executeWaitJoin(
	params: { target?: string; timeout_seconds?: number },
	signal?: AbortSignal,
) {
	const { chatId, parentKind, parentId } = crackContext();
	const budget = clamp(params.timeout_seconds ?? 600, 5, 3600);
	const deadline = Date.now() + budget * 1000;
	const graceDeadline = Date.now() + 10_000; // a spawn may race the first poll
	const results = new Map<string, WaitResult>();
	const strikes = new Map<string, number>();
	let pending: WaitPending[] = [];
	let failureSince: number | null = null;

	while (true) {
		// Two-strike rule: a target notified=true with no inbox entry on two
		// consecutive polls is in the transient finish() gap (or was consumed
		// earlier) — ask the server to rebuild its entry from run state.
		const rebuild = [...strikes.entries()]
			.filter(([, n]) => n >= 2)
			.map(([id]) => id);
		const remaining = deadline - Date.now();
		if (remaining <= 0) {
			const what = pending
				.map((p) => `${p.run_id} (${p.persona}, ${p.phase})`)
				.join(", ");
			const text =
				(results.size
					? [...results.values()].map((r) => r.text).join("\n\n---\n\n") +
						"\n\n---\n\n"
					: "") +
				`Still running: ${what}. Call wait_join again to keep waiting (free) ` +
				"or end your turn — results arrive automatically.";
			return { content: [{ type: "text" as const, text: truncateTail(text).content }] };
		}

		const block = Math.min(25, remaining / 1000);
		let data: WaitResponse;
		try {
			const to = signal
				? AbortSignal.any([signal, AbortSignal.timeout(Math.ceil(block) * 1000 + 10000)])
				: AbortSignal.timeout(Math.ceil(block) * 1000 + 10000);
			const res = await fetch(
				`${BASE}/api/chats/${encodeURIComponent(chatId)}/sub_agents/wait`,
				{
					method: "POST",
					headers: { "Content-Type": "application/json" },
					body: JSON.stringify({
						parent_kind: parentKind,
						parent_id: parentId,
						target: params.target,
						rebuild,
						block_seconds: block,
					}),
					signal: to,
				},
			);
			if (!res.ok) {
				throw new Error(truncateTail(await res.text()).content);
			}
			data = (await res.json()) as WaitResponse;
			failureSince = null;
		} catch (e) {
			if (signal?.aborted) throw e;
			// Tolerate ~30s of transient fetch failures (server reload mid-wait).
			failureSince ??= Date.now();
			if (Date.now() - failureSince > 30_000) {
				throw new Error(
					`crack-server unreachable at ${BASE} for 30s: ${e instanceof Error ? (e.cause ?? e.message) : e}`,
				);
			}
			await sleep(1000);
			continue;
		}

		for (const r of data.results) results.set(r.run_id, r);
		pending = data.pending;
		for (const p of pending) {
			strikes.set(p.run_id, p.notified ? (strikes.get(p.run_id) ?? 0) + 1 : 0);
		}

		if (pending.length === 0) {
			if (results.size === 0) {
				if (Date.now() < graceDeadline) {
					await sleep(1000);
					continue;
				}
				const text =
					"No outstanding sub-agents" +
					(params.target ? ` matching ${JSON.stringify(params.target)}` : "") +
					". Spawn one first, or check the run id/persona slug.";
				return { content: [{ type: "text" as const, text }] };
			}
			const parts = [...results.values()].map((r) =>
				r.delivered_earlier ? `(delivered earlier)\n${r.text}` : r.text,
			);
			return {
				content: [
					{ type: "text" as const, text: truncateTail(parts.join("\n\n---\n\n")).content },
				],
			};
		}
	}
}

function findSubAgentsDir(): string | null {
	// Prefer walking up from cwd (the server pins pi's cwd to the project
	// root); fall back to this file's location
	// (.pi/extensions/crack/index.ts -> ../../crack/sub_agents).
	for (let d = process.cwd(); ; d = dirname(d)) {
		const p = join(d, ".pi/crack/sub_agents");
		if (existsSync(p)) return p;
		if (dirname(d) === d) break;
	}
	const self = join(dirname(fileURLToPath(import.meta.url)), "../../crack/sub_agents");
	return existsSync(self) ? self : null;
}

export default function crack(pi: ExtensionAPI) {
	try {
		const depth = Number.parseInt(process.env.CRACK_SUBAGENT_DEPTH ?? "0", 10) || 0;
		const canSpawn = depth < MAX_DEPTH;
		// Todo list — the plan the prewalk swap and nudges key off. State is
		// reconstructed from the session tree (branch-safe) and always echoed as
		// plain text so crack-server can read it out of the persisted tool_block.
		let todos: Todo[] = [];
		const reconstructTodos = (ctx: ExtensionContext) => {
			todos = [];
			for (const entry of ctx.sessionManager.getBranch()) {
				if (entry.type !== "message") continue;
				const msg = entry.message;
				if (msg.role !== "toolResult" || msg.toolName !== "todo") continue;
				const details = msg.details as TodoDetails | undefined;
				if (details?.todos) todos = details.todos;
			}
		};
		pi.on("session_start", async (_event, ctx) => reconstructTodos(ctx));
		pi.on("session_tree", async (_event, ctx) => reconstructTodos(ctx));

		if (canSpawn) {
			pi.on("tool_call", async (event, _ctx) => {
				if (!isDestructive(event.toolName)) return;
				if (!(await hasRunningChildren())) return;
				await waitForAllChildren();
			});
		}

		pi.registerTool({
			name: "todo",
			label: "Todo list",
			description:
				"Manage your plan as a todo list. action=write replaces the whole list with `items` " +
				`(use it once, right after you finish planning; keep it to ~${TODO_MAX} concrete, ` +
				"independently-verifiable steps). action=toggle flips item `id` done/undone as you " +
				"complete it. action=list re-shows it. Keep it updated as you work — it is your " +
				"checklist, not a file to write.",
			parameters: TODO_PARAMS,
			executionMode: "parallel",
			async execute(_id, params) {
				const action = String(params.action || "").toLowerCase();
				if (action === "write") {
					todos = (params.items ?? [])
						.slice(0, TODO_MAX)
						.map((t, i) => ({ id: i + 1, text: String(t), done: false }));
				} else if (action === "toggle") {
					const t = todos.find((x) => x.id === params.id);
					if (!t) {
						return {
							content: [
								{ type: "text" as const, text: `No todo #${params.id}.\n${renderTodos(todos)}` },
							],
						};
					}
					t.done = !t.done;
				} else if (action !== "list") {
					return {
						content: [
							{
								type: "text" as const,
								text: `Unknown action ${JSON.stringify(params.action)}. Use write | toggle | list.`,
							},
						],
					};
				}
				return {
					content: [{ type: "text" as const, text: renderTodos(todos) }],
					details: { todos: [...todos] } as TodoDetails,
				};
			},
		});
		if (canSpawn) {
			pi.registerTool({
				name: "wait_join",
				label: "Wait for sub-agents",
				description:
					"Block until spawned sub-agents finish and return their reports as the tool result. " +
					"Waiting is free (no tokens burned, no polling). Always prefer this over checking " +
					"report files — never poll report.md with bash sleep loops.",
				parameters: WAIT_PARAMS,
				executionMode: "parallel",
				async execute(_id, params, signal) {
					return executeWaitJoin(params, signal);
				},
			});
		}
		pi.registerTool({
			name: "ask_user",
			label: "Ask the human",
			description:
				"Ask the human a question and suspend until they answer. The session ends its " +
				"current turn cleanly (nothing burns tokens or times out while waiting, even " +
				"for hours); a fresh hop resumes with the answer. After calling this, end " +
				"your turn immediately — make no further tool calls.",
			parameters: ASK_PARAMS,
			executionMode: "parallel",
			async execute(_id, params, signal) {
				const { chatId, parentKind, parentId } = crackContext();
				const to = signal
					? AbortSignal.any([signal, AbortSignal.timeout(15000)])
					: AbortSignal.timeout(15000);
				let res: Response;
				try {
					res = await fetch(
						`${BASE}/api/chats/${encodeURIComponent(chatId)}/ask_user`,
						{
							method: "POST",
							headers: { "Content-Type": "application/json" },
							body: JSON.stringify({
								parent_kind: parentKind,
								parent_id: parentId,
								question: params.question,
								choices: params.choices,
							}),
							signal: to,
						},
					);
				} catch (e) {
					throw new Error(
						`crack-server unreachable at ${BASE}: ${e instanceof Error ? (e.cause ?? e.message) : e}`,
					);
				}
				if (!res.ok) {
					throw new Error(truncateTail(await res.text()).content);
				}
				const text =
					"Question recorded. This session suspends until the user answers — " +
					"end your turn now, make no further tool calls.";
				return { content: [{ type: "text" as const, text }] };
			},
		});
		pi.registerTool({
			name: "analyze_image",
			label: "Analyze image(s)",
			description:
				"Analyze one or more image files with a vision model. Takes a prompt plus a list of " +
				"image paths; returns the vision model's answer as text. Paths must exist and be " +
				"valid images — the server rejects the call listing any bad paths.",
			parameters: ANALYZE_IMAGE_PARAMS,
			executionMode: "parallel",
			async execute(_id, params, signal) {
				const missing = params.image_paths.filter((p) => !existsSync(p));
				if (missing.length > 0) {
					throw new Error(`image path(s) not found: ${missing.join(", ")}`);
				}
				const to = signal
					? AbortSignal.any([signal, AbortSignal.timeout(600_000)])
					: AbortSignal.timeout(600_000);
				let res: Response;
				try {
					res = await fetch(`${BASE}/api/vision/analyze`, {
						method: "POST",
						headers: { "Content-Type": "application/json" },
						body: JSON.stringify({
							prompt: params.prompt,
							image_paths: params.image_paths,
						}),
						signal: to,
					});
				} catch (e) {
					throw new Error(
						`crack-server unreachable at ${BASE}: ${e instanceof Error ? (e.cause ?? e.message) : e}`,
					);
				}
				if (!res.ok) {
					throw new Error(truncateTail(await res.text()).content);
				}
				const d = (await res.json()) as { text: string };
				return { content: [{ type: "text" as const, text: d.text }] };
			},
		});
		const dir = findSubAgentsDir();
		if (!dir) return;
		if (!canSpawn) return;
		for (const ent of readdirSync(dir, { withFileTypes: true })) {
			if (!ent.isDirectory()) continue;
			const slug = ent.name;
			let cfg: { tool_description?: string; tool_label?: string };
			try {
				cfg = JSON.parse(readFileSync(join(dir, slug, "config.json"), "utf8"));
			} catch (e) {
				console.error(`crack: skip persona ${slug}: ${e}`);
				continue;
			}
			pi.registerTool({
				name: `spawn_${slug}`,
				label: cfg.tool_label ?? slug,
				description:
					(cfg.tool_description ?? `Spawn ${slug} sub-agent.`) +
					" Runs in the background. Call wait_join to block until it finishes and get its report; do not poll report files.",
				parameters: PARAMS,
				executionMode: "parallel",
				async execute(_id, params, signal) {
					const { chatId, parentKind, parentId } = crackContext();
					let sawSlotPending = false;
					let waited = false;
					while (true) {
						const to = signal
							? AbortSignal.any([signal, AbortSignal.timeout(12_000)])
							: AbortSignal.timeout(12_000);
						let res: Response;
						try {
							res = await fetch(
								`${BASE}/api/chats/${encodeURIComponent(chatId)}/sub_agents/spawn`,
								{
									method: "POST",
									headers: { "Content-Type": "application/json" },
									body: JSON.stringify({
										persona: slug,
										instructions: params.instructions,
										parent_kind: parentKind,
										parent_id: parentId,
										depth,
										plan: params.plan,
									}),
									signal: to,
								},
							);
						} catch (e) {
							throw new Error(
								`crack-server unreachable at ${BASE}: ${e instanceof Error ? (e.cause ?? e.message) : e}`,
							);
						}
						if (!res.ok) {
							throw new Error(truncateTail(await res.text()).content);
						}
						const d = (await res.json()) as SpawnResult;
						if (d.status === "slot_pending") {
							sawSlotPending = true;
							if (signal?.aborted) {
								throw new Error("spawn cancelled while waiting for a free slot");
							}
							await sleep(1000);
							continue;
						}
						if (d.waited) {
							waited = true;
						}
						let prefix = "";
						if (sawSlotPending || waited) {
							prefix = `⏳ waited for a free slot (max ${MAX_PARALLEL_SUBAGENTS} parallel).\n`;
						}
						const text = truncateTail(
							`${prefix}Spawned ${slug} run ${d.run_id}. It runs in the background: call wait_join (target "${d.run_id}", or omit for all) to block until it finishes and receive its report, or end your turn and it will report back automatically. Do NOT poll ${d.report_path} with bash sleeps.`,
						).content;
						return { content: [{ type: "text", text }] };
					}
				},
			});
		}
	} catch (e) {
		// Never crash pi at load time — a broken extension dir means no tools.
		console.error(`crack: extension disabled (${e})`);
	}
}
