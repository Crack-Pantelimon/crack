/**
 * crack_pi — /crack* commands and prompt editor server integration.
 *
 * Prompt files: .pi/crack/tasks/<task_id>/*.md (globbed on each page load).
 * Server: cd .pi/crack/server && uv run crack-server
 */

import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const SERVER_SUBDIR = join(".pi", "crack", "server");
const TASKS_SUBDIR = join(".pi", "crack", "tasks");
const DEFAULT_PORT = 9847;

function tasksRoot(cwd: string): string {
	return join(cwd, TASKS_SUBDIR);
}

function listTaskIds(cwd: string): string[] {
	const root = tasksRoot(cwd);
	if (!existsSync(root)) {
		return [];
	}
	return readdirSync(root, { withFileTypes: true })
		.filter((d) => d.isDirectory())
		.map((d) => d.name)
		.sort();
}

function serverUrl(taskId: string, port = DEFAULT_PORT): string {
	return `http://127.0.0.1:${port}/tasks/${encodeURIComponent(taskId)}`;
}

function parseCrackArgs(raw: string): { sub: string; rest: string } {
	const trimmed = raw.trim();
	if (!trimmed) {
		return { sub: "help", rest: "" };
	}
	const [sub, ...tail] = trimmed.split(/\s+/);
	return { sub: sub.toLowerCase(), rest: tail.join(" ").trim() };
}

async function openInBrowser(pi: ExtensionAPI, url: string, signal?: AbortSignal): Promise<void> {
	if (process.platform === "darwin") {
		await pi.exec("open", [url], { signal });
		return;
	}
	if (process.platform === "win32") {
		await pi.exec("cmd", ["/c", "start", "", url], { signal });
		return;
	}
	await pi.exec("xdg-open", [url], { signal });
}

const HELP_TEXT = [
	"/crack — this help",
	"/crack tasks — list task ids",
	"/crack open <task_id> — open prompt editor in browser",
	"/crack server — how to start the Python server",
	`/crack url <task_id> — print editor URL (port ${DEFAULT_PORT})`,
].join("\n");

function taskCompletions(prefix: string, cwd: string) {
	const tasks = listTaskIds(cwd);
	const filtered = tasks.filter((t) => t.startsWith(prefix));
	return filtered.length > 0 ? filtered.map((t) => ({ value: t, label: t })) : null;
}

async function handleCrackSubcommand(
	pi: ExtensionAPI,
	sub: string,
	rest: string,
	cwd: string,
	ctx: { ui: { notify: (m: string, t?: "info" | "error") => void } },
): Promise<void> {
	switch (sub) {
		case "help":
		case "h":
			ctx.ui.notify(HELP_TEXT, "info");
			return;
		case "tasks":
		case "list": {
			const tasks = listTaskIds(cwd);
			if (tasks.length === 0) {
				ctx.ui.notify(`No tasks under ${TASKS_SUBDIR}/`, "info");
				return;
			}
			ctx.ui.notify(tasks.map((t) => `• ${t}`).join("\n"), "info");
			return;
		}
		case "server":
			ctx.ui.notify(
				`From project root:\n  cd ${SERVER_SUBDIR} && uv sync && uv run crack-server\n\nEnv: CRACK_PI_PROJECT_ROOT, CRACK_PI_PORT`,
				"info",
			);
			return;
		case "url": {
			const taskId = rest;
			if (!taskId) {
				ctx.ui.notify("Usage: /crack url <task_id>", "error");
				return;
			}
			ctx.ui.notify(serverUrl(taskId), "info");
			return;
		}
		case "open": {
			const taskId = rest;
			if (!taskId) {
				ctx.ui.notify("Usage: /crack open <task_id>", "error");
				return;
			}
			const url = serverUrl(taskId);
			try {
				await openInBrowser(pi, url);
				ctx.ui.notify(`Opened ${url}`, "info");
			} catch (e) {
				ctx.ui.notify(`Open failed (${e}). URL: ${url}`, "error");
			}
			return;
		}
		default:
			ctx.ui.notify(HELP_TEXT, "info");
	}
}

export default function crackPiExtension(pi: ExtensionAPI) {
	pi.registerCommand("crack", {
		description: "Crack task prompts: tasks | open | server | url",
		getArgumentCompletions: (prefix, ctx) => {
			const verbs = ["tasks", "open", "server", "url", "help"];
			if (!prefix || verbs.some((v) => v.startsWith(prefix))) {
				const hit = verbs.filter((v) => v.startsWith(prefix));
				if (hit.length > 0) {
					return hit.map((v) => ({ value: v, label: v }));
				}
			}
			return taskCompletions(prefix, ctx.cwd);
		},
		handler: async (args, ctx) => {
			const { sub, rest } = parseCrackArgs(args);
			await handleCrackSubcommand(pi, sub, rest, ctx.cwd, ctx);
		},
	});

	pi.registerCommand("crack-tasks", {
		description: "List crack task ids under .pi/crack/tasks/",
		handler: async (_args, ctx) => {
			await handleCrackSubcommand(pi, "tasks", "", ctx.cwd, ctx);
		},
	});

	pi.registerCommand("crack-open", {
		description: "Open crack prompt editor for a task",
		getArgumentCompletions: (prefix, ctx) => taskCompletions(prefix, ctx.cwd),
		handler: async (args, ctx) => {
			await handleCrackSubcommand(pi, "open", args.trim(), ctx.cwd, ctx);
		},
	});

	pi.registerCommand("crack-server", {
		description: "Show how to run the crack prompt editor server",
		handler: async (_args, ctx) => {
			await handleCrackSubcommand(pi, "server", "", ctx.cwd, ctx);
		},
	});
}
