/**
 * crack — spawn background sub-agents via crack-server personas.
 *
 * Tools-only (no slash commands). Personas are read synchronously from
 * .pi/crack/sub_agents/<slug>/config.json at factory time — no HTTP on the
 * registration path. Chat context (CRACK_CHAT_ID / CRACK_PARENT_* /
 * CRACK_SUBAGENT_DEPTH) is checked in execute(), so the tools are visible in
 * every pi session but throw a clear error outside a crack chat/sub-agent run.
 * Rigid pipeline stages stay isolated via their explicit --tools allowlists.
 *
 * Server: http://127.0.0.1:9847 (override with CRACK_PI_PORT)
 */

import { readdirSync, readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { truncateTail } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

const BASE = `http://127.0.0.1:${process.env.CRACK_PI_PORT ?? "9847"}`;
const MAX_DEPTH = 3;

const PARAMS = Type.Object({
	instructions: Type.String({ description: "Task for the sub-agent" }),
});

interface SpawnResult {
	run_id: string;
	report_path: string;
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
		const dir = findSubAgentsDir();
		if (!dir) return;
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
					" Returns immediately; runs in the background and reports back here when done.",
				parameters: PARAMS,
				executionMode: "parallel",
				async execute(_id, params, signal) {
					const chatId = process.env.CRACK_CHAT_ID;
					if (!chatId) {
						throw new Error(
							"spawn tools only work inside a crack unscripted chat or sub-agent run",
						);
					}
					const depth = Number.parseInt(process.env.CRACK_SUBAGENT_DEPTH ?? "0", 10) || 0;
					if (depth >= MAX_DEPTH) {
						throw new Error(`max sub-agent depth (${MAX_DEPTH}) reached`);
					}
					const to = signal
						? AbortSignal.any([signal, AbortSignal.timeout(15000)])
						: AbortSignal.timeout(15000);
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
									parent_kind: process.env.CRACK_PARENT_KIND ?? "chat",
									parent_id: process.env.CRACK_PARENT_ID ?? chatId,
									depth,
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
					const text = truncateTail(
						`Spawned ${slug} run ${d.run_id}. It runs in the background and will report back here when done; report path: ${d.report_path}.`,
					).content;
					return { content: [{ type: "text", text }] };
				},
			});
		}
	} catch (e) {
		// Never crash pi at load time — a broken extension dir means no tools.
		console.error(`crack: extension disabled (${e})`);
	}
}
