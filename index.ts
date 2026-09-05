/**
 * omp-openrouter-image — OpenRouter Image API as OMP v18 extension tool.
 *
 * Generates images via OpenRouter's GPT Image models. Returns base64 image data
 * from the API, decodes it and writes the file to ~/.omp/agent/generated-images/.
 *
 * Tool name:    xd://openrouter_image
 * Env var:      OPENROUTER_API_KEY  (Bearer token from openrouter.ai/keys)
 * Output dir:   ~/.omp/agent/generated-images/openrouter-<unix>-<n>.<ext>
 *
 * Example call (JSON):
 *   {
 *     "prompt": "A minimal blue circle on a white background",
 *     "model": "openai/gpt-image-1-mini",
 *     "aspect_ratio": "16:9",
 *     "reference_image": "/tmp/sketch.png"
 *   }
 *
 * Supported models (2026-09):
 *   - openai/gpt-image-2          (newest, 9 aspect ratios)
 *   - openai/gpt-image-1          (4 ratios, transparent bg)
 *   - openai/gpt-image-1-mini     (cheap)
 *   - openai/gpt-5-image         (reasoning + image)
 *   - openai/gpt-5-image-mini    (reasoning + image, cheap)
 *   - openai/gpt-5.4-image-2
 *
 * Endpoint:     POST https://openrouter.ai/api/v1/images
 *               Body: { model, prompt, n?, quality?, background?, output_format?,
 *                       output_compression?, resolution?, aspect_ratio?, size?,
 *                       seed?, stream?, input_references? }
 *               Response: { created, data: [{ b64_json, media_type }], usage }
 *
 * Reference image handling:
 *   - The tool accepts a local file path only (PNG/JPG/JPEG/GIF/WEBP).
 *   - The extension reads the file, detects MIME via extension, base64-encodes
 *     it, and forwards it as `input_references` to the model.
 *   - The prompt is prefixed with a short context-instruction so the model
 *     treats the reference as visual context, not as an edit target.
 */

import { join, extname } from "node:path";
import { homedir } from "node:os";
import { mkdir, writeFile, readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent/extensibility/extensions/types";
import type { AgentToolResult, AgentToolUpdateCallback, ToolRenderResultOptions } from "@oh-my-pi/pi-coding-agent/extensibility/extensions/types";
import type { Theme } from "@oh-my-pi/pi-coding-agent/modes/theme/theme";
import type { TextContent } from "@oh-my-pi/pi-ai";
import type { Component } from "@oh-my-pi/pi-tui";
import { Box, Text } from "@oh-my-pi/pi-tui";
const OPENROUTER_IMAGE_URL = "https://openrouter.ai/api/v1/images";
const DEFAULT_MODEL = "openai/gpt-image-2";
const DEFAULT_ASPECT_RATIO = "16:9";
const DEFAULT_OUTPUT_FORMAT = "png";
const DEFAULT_TIMEOUT_MS = 120_000;
const GENERATED_DIR = join(homedir(), ".omp", "agent", "generated-images");
const IMAGE_GEN_SETTINGS_FILE = join(homedir(), ".omp", "agent", "image-gen.json");
const REFERENCE_PROMPT_PREFIX =
	"Use the provided reference image as visual context. The reference image shows:\n\n";
const MIME_BY_EXT: Record<string, string> = {
	".png": "image/png",
	".jpg": "image/jpeg",
	".jpeg": "image/jpeg",
	".gif": "image/gif",
	".webp": "image/webp",
};

const ALLOWED_ASPECT_RATIOS = [
	"1:1",
	"3:2",
	"2:3",
	"4:3",
	"3:4",
	"16:9",
	"9:16",
	"21:9",
	"auto",
] as const;

const HTTP_ERROR_MESSAGES: Record<number, string> = {
	400: "OpenRouter rejected the request",
	401: "Invalid or missing OpenRouter API key",
	402: "Insufficient OpenRouter credits",
	403: "Forbidden — check model access permissions",
	500: "OpenRouter server error",
	502: "OpenRouter gateway error",
	503: "OpenRouter service unavailable",
	504: "OpenRouter gateway timeout",
};

export type OpenrouterImageParams = {
	prompt: string;
	model?: string;
	reference_image?: string;
	aspect_ratio?: string;
	quality?: string;
	background?: string;
	output_format?: string;
	resolution?: string;
	n?: number;
	seed?: number;
	input_references?: Array<{ type: string; image_url: { url: string } }>;
};

export interface OpenrouterImageDetails {
	savedPaths: string[];
	mediaType: string;
	b64Length: number;
	usage?: {
		prompt_tokens?: number;
		completion_tokens?: number;
		total_tokens?: number;
		cost?: number;
	};
	cost?: number;
	model: string;
	requestedN: number;
	liveStatus?: string;
	elapsedSeconds?: number;
}

export interface OpenrouterImageErrorDetails {
	savedPaths: string[];
	mediaType: null;
	b64Length: number;
	usage: undefined;
	cost: undefined;
	model: string;
	requestedN: number;
	liveStatus?: string;
	elapsedSeconds?: number;
	errorStatus?: number;
}

interface OpenrouterImageResponse {
	created?: number;
	data?: Array<{ b64_json?: string; media_type?: string }>;
	usage?: {
		prompt_tokens?: number;
		completion_tokens?: number;
		total_tokens?: number;
		cost?: number;
	};
	error?: {
		message?: string;
		code?: string | number;
		type?: string;
	};
}
function detectMimeFromPath(filePath: string): string {
	return MIME_BY_EXT[extname(filePath).toLowerCase()] ?? "image/png";
}

async function resolveReferenceImage(ref: string): Promise<string> {
	if (/^(data:|https?:)/i.test(ref)) return ref;
	const data = await readFile(ref);
	const mime = detectMimeFromPath(ref);
	return `data:${mime};base64,${data.toString("base64")}`;
}

function truncate(str: string, maxLen: number): string {
	if (str.length <= maxLen) return str;
	return `${str.slice(0, Math.max(0, maxLen - 1))}…`;
}

function stripControls(s: string): string {
	return s.replace(/[\x00-\x08\x0b-\x1f\x7f-\x9f]/g, "");
}

function textContent(text: string): TextContent {
	return { type: "text", text };
}

function delayMs(ms: number): Promise<void> {
	return new Promise<void>((resolve) => setTimeout(resolve, ms));
}
async function loadImageGenSettings(): Promise<Record<string, unknown> | null> {
	try {
		const raw = await readFile(IMAGE_GEN_SETTINGS_FILE, "utf8");
		const trimmed = raw.trim();
		if (trimmed.length === 0) return null;
		const parsed = JSON.parse(trimmed) as unknown;
		if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
			return parsed as Record<string, unknown>;
		}
		return null;
	} catch (err) {
		const e = err as NodeJS.ErrnoException;
		if (e.code === "ENOENT") return null;
		throw err;
	}
}

async function resolveOpenrouterApiKey(): Promise<string | null> {
	const fromEnv = process.env.OPENROUTER_API_KEY?.trim();
	if (fromEnv && fromEnv.length > 0) return fromEnv;
	try {
		const settings = await loadImageGenSettings();
		if (!settings) return null;
		const candidates = [settings.apiKey, settings.OPENROUTER_API_KEY, settings.openrouter_api_key];
		for (const value of candidates) {
			if (typeof value === "string" && value.trim().length > 0) {
				return value.trim();
			}
		}
		return null;
	} catch (err) {
		throw new Error(
			`Failed to read ${IMAGE_GEN_SETTINGS_FILE}: ${err instanceof Error ? err.message : String(err)}`,
		);
	}
}

function okResult(
	text: string,
	details: OpenrouterImageDetails,
): AgentToolResult<OpenrouterImageDetails> {
	return {
		content: [textContent(stripControls(text))],
		details,
		isError: false,
	};
}

function errResult(
	message: string,
	details: OpenrouterImageErrorDetails,
): AgentToolResult<OpenrouterImageDetails> {
	return {
		content: [textContent(stripControls(message))],
		details: details as unknown as OpenrouterImageDetails,
		isError: true,
	};
}

async function callImageApi(
  params: OpenrouterImageParams,
  apiKey: string,
  onProgress?: (elapsedSeconds: number) => void,
  externalSignal?: AbortSignal,
): Promise<OpenrouterImageResponse> {
	const body: Record<string, unknown> = {
		model: params.model ?? DEFAULT_MODEL,
		prompt: params.prompt,
	};

	if (params.n !== undefined) body.n = params.n;
	if (params.quality) body.quality = params.quality;
	if (params.background) body.background = params.background;
	if (params.output_format) body.output_format = params.output_format;
	if (params.resolution) body.resolution = params.resolution;
	if (params.aspect_ratio) body.aspect_ratio = params.aspect_ratio;
	if (params.seed !== undefined) body.seed = params.seed;
	if (params.input_references && params.input_references.length > 0) {
		body.input_references = params.input_references;
	}

	const headers: Record<string, string> = {
		Authorization: `Bearer ${apiKey}`,
		"Content-Type": "application/json",
		Accept: "application/json",
	};
	if (process.env.OPENROUTER_HTTP_REFERER) headers["HTTP-Referer"] = process.env.OPENROUTER_HTTP_REFERER;
	if (process.env.OPENROUTER_X_TITLE) headers["X-Title"] = process.env.OPENROUTER_X_TITLE;

	const callStartedAt = Date.now();
	let progressTimer: NodeJS.Timeout | undefined;
	if (onProgress) {
		progressTimer = setInterval(() => {
			const secs = Math.floor((Date.now() - callStartedAt) / 1000);
			try { onProgress(secs); } catch { /* swallow progress callback errors */ }
		}, 1000);
	}
	let lastError: Error = new Error("All retry attempts failed");
	const maxAttempts = 2;

	const stopTimer = () => {
		if (progressTimer) { clearInterval(progressTimer); progressTimer = undefined; }
	};

	for (let attempt = 0; attempt < maxAttempts; attempt++) {
		const controller = new AbortController();
		const timeoutId = setTimeout(() => controller.abort(), DEFAULT_TIMEOUT_MS);
		const onExternalAbort = () => controller.abort();
		if (externalSignal) {
			if (externalSignal.aborted) controller.abort();
			else externalSignal.addEventListener("abort", onExternalAbort, { once: true });
		}

		try {
			const response = await fetch(OPENROUTER_IMAGE_URL, {
				method: "POST",
				headers,
				body: JSON.stringify(body),
				signal: controller.signal,
			});
			clearTimeout(timeoutId);
			if (externalSignal) externalSignal.removeEventListener("abort", onExternalAbort);
			if (response.status >= 500 && response.status < 600 && attempt < maxAttempts - 1) {
				await delayMs(Math.pow(2, attempt + 1) * 1000);
				continue;
			}

			const text = await response.text();
			const parsed = text.length > 0 ? (JSON.parse(text) as OpenrouterImageResponse) : ({});

			if (!response.ok) {
				const apiMessage = parsed.error?.message ?? "";
				const statusLabel = HTTP_ERROR_MESSAGES[response.status] ?? `HTTP ${response.status}`;
				if (response.status === 401) {
					throw new Error(`${statusLabel} — set OPENROUTER_API_KEY to a valid sk-or-… key`);
				}
				if (response.status === 402) {
					throw new Error(`${statusLabel} — top up at openrouter.ai/credits`);
				}
				if (response.status === 429) {
					throw new Error(`${statusLabel} — slow down or reduce n/quality`);
				}
				throw new Error(apiMessage ? `${statusLabel}: ${apiMessage}` : statusLabel);
			}

			stopTimer();
			return parsed;
		} catch (err) {
			clearTimeout(timeoutId);
			stopTimer();
			if (externalSignal) externalSignal.removeEventListener("abort", onExternalAbort);
			lastError = err instanceof Error ? err : new Error(String(err));
			if (lastError.name === "AbortError") {
				lastError = new Error(`Request timeout after ${DEFAULT_TIMEOUT_MS}ms`);
			}
			if (attempt >= maxAttempts - 1) break;
		}
	}
	stopTimer();
	throw lastError;
}

async function executeOpenrouterImage(
  params: OpenrouterImageParams,
  onUpdate?: AgentToolUpdateCallback<OpenrouterImageDetails>,
  signal?: AbortSignal,
): Promise<AgentToolResult<OpenrouterImageDetails>> {
	const model = params.model ?? DEFAULT_MODEL;
	const requestedN = params.n ?? 1;
	const callStart = Date.now();
	let lastElapsed = 0;
	const baseDetails: OpenrouterImageDetails = {
		savedPaths: [] as string[],
		mediaType: "image/png",
		b64Length: 0,
		usage: undefined,
		cost: undefined,
		model,
		requestedN,
		liveStatus: "queued",
		elapsedSeconds: 0,
	};
	const fireUpdate = () => {
		if (!onUpdate) return;
		try {
			onUpdate({
				content: [textContent(stripControls(`⏳ generating… ${lastElapsed}s`))],
				details: { ...baseDetails, liveStatus: "generating", elapsedSeconds: lastElapsed },
				isError: false,
			});
		} catch { /* update callback errors are non-fatal */ }
	};

	try {
		const apiKey = await resolveOpenrouterApiKey();
		if (!apiKey || apiKey.trim().length === 0) {
			return errResult(
				`OpenRouter API key not configured. Set OPENROUTER_API_KEY env var or write {"apiKey":"sk-or-…"} to ${IMAGE_GEN_SETTINGS_FILE} (chmod 600).`,
				{ ...baseDetails, liveStatus: "error", errorStatus: 401 } as unknown as OpenrouterImageErrorDetails,
			);
		}

		if (params.aspect_ratio && !ALLOWED_ASPECT_RATIOS.includes(params.aspect_ratio as typeof ALLOWED_ASPECT_RATIOS[number])) {
			return errResult(
				`Invalid aspect_ratio "${params.aspect_ratio}". Allowed: ${ALLOWED_ASPECT_RATIOS.join(", ")}`,
				{ ...baseDetails, liveStatus: "error", errorStatus: 400 } as unknown as OpenrouterImageErrorDetails,
			);
		}

		let effectivePrompt = params.prompt;
		let references: Array<{ type: string; image_url: { url: string } }> | undefined;
		if (params.reference_image && params.reference_image.trim().length > 0) {
			const url = await resolveReferenceImage(params.reference_image);
			references = [{ type: "image_url", image_url: { url } }];
			effectivePrompt = `${REFERENCE_PROMPT_PREFIX}${params.prompt}`;
		}

		const apiParams: OpenrouterImageParams = {
			...params,
			prompt: effectivePrompt,
			...(references ? { input_references: references } : {}),
		};

		baseDetails.liveStatus = "generating";
		fireUpdate();
		const onProgress = (secs: number) => {
			lastElapsed = secs;
			baseDetails.elapsedSeconds = secs;
			fireUpdate();
		};
	const data = await callImageApi(apiParams, apiKey, onProgress, signal);

	const images = data.data ?? [];
	if (images.length === 0) {
		return errResult(
			"OpenRouter returned no image data",
			{ ...baseDetails, liveStatus: "error", errorStatus: 502 } as unknown as OpenrouterImageErrorDetails,
		);
	}

		if (!existsSync(GENERATED_DIR)) {
			await mkdir(GENERATED_DIR, { recursive: true });
		}

		const timestamp = data.created ?? Math.floor(Date.now() / 1000);
		const ext = params.output_format ?? DEFAULT_OUTPUT_FORMAT;
		const savedPaths: string[] = [];
		let mediaType = "image/png";
		let totalB64 = 0;

		for (let idx = 0; idx < images.length; idx++) {
			const img = images[idx];
			if (!img?.b64_json) continue;
			const buf = Buffer.from(img.b64_json, "base64");
			const filename = `openrouter-${timestamp}-${idx + 1}.${ext}`;
			const fullPath = join(GENERATED_DIR, filename);
			await writeFile(fullPath, buf);
			savedPaths.push(fullPath);
			totalB64 += img.b64_json.length;
			if (img.media_type) mediaType = img.media_type;
		}

		if (savedPaths.length === 0) {
			return errResult(
				"OpenRouter response had empty b64_json fields",
			{ ...baseDetails, errorStatus: 502 } as unknown as OpenrouterImageErrorDetails,
			);
		}
	const usage = data.usage;
	const totalElapsed = Math.floor((Date.now() - callStart) / 1000);
	const details: OpenrouterImageDetails = {
		savedPaths,
		mediaType,
		b64Length: totalB64,
		usage,
		cost: usage?.cost,
		model,
		requestedN,
		liveStatus: "done",
		elapsedSeconds: totalElapsed,
	};
	const lines: string[] = [];
	lines.push(`✓ Generated ${savedPaths.length} image(s) with ${model}`);
	for (const p of savedPaths) {
			lines.push(`   ${p}`);
		}
		lines.push(`   media_type: ${mediaType}`);
		lines.push(`   b64 length: ${totalB64} chars`);
		if (usage) {
			const parts: string[] = [];
			if (usage.prompt_tokens !== undefined) parts.push(`prompt=${usage.prompt_tokens}`);
			if (usage.completion_tokens !== undefined) parts.push(`completion=${usage.completion_tokens}`);
			if (usage.total_tokens !== undefined) parts.push(`total=${usage.total_tokens}`);
			if (usage.cost !== undefined) parts.push(`cost=$${usage.cost.toFixed(6)}`);
			if (parts.length > 0) lines.push(`   usage: ${parts.join(", ")}`);
		}
		lines.push(`   output_dir: ${GENERATED_DIR}`);

		return okResult(lines.join("\n"), details);
	} catch (err) {
		const message = err instanceof Error ? err.message : String(err);
		return errResult(message, { ...baseDetails, errorStatus: undefined } as unknown as OpenrouterImageErrorDetails);
	}
}
function wrapInBox(inner: Text, theme: Theme, borderColor: "muted" | "toolTitle" | "error" | "success" | "accent" = "muted"): Box {
	const box = new Box(1, 0, undefined, {
		chars: {
			topLeft: theme.boxRound.topLeft,
			topRight: theme.boxRound.topRight,
			bottomLeft: theme.boxRound.bottomLeft,
			bottomRight: theme.boxRound.bottomRight,
			horizontal: theme.boxRound.horizontal,
			vertical: theme.boxRound.vertical,
		},
		color: (s: string) => theme.fg(borderColor, s),
	});
	box.addChild(inner);
	return box;
}

export function renderCall(
  args: OpenrouterImageParams,
  _options: ToolRenderResultOptions,
  theme: Theme,
): Component {
	const prompt = stripControls(args.prompt ?? "");
	const model = args.model ?? DEFAULT_MODEL;
	const meta: string[] = [`model=${model}`];
	if (args.aspect_ratio && args.aspect_ratio !== DEFAULT_ASPECT_RATIO) {
		meta.push(`ar=${args.aspect_ratio}`);
	}
	if (args.quality && args.quality !== "auto") meta.push(`q=${args.quality}`);
	if (args.resolution) meta.push(`res=${args.resolution}`);
	if (args.n && args.n > 1) meta.push(`n=${args.n}`);
	if (args.background) meta.push(`bg=${args.background}`);
	if (args.output_format && args.output_format !== DEFAULT_OUTPUT_FORMAT) {
		meta.push(`fmt=${args.output_format}`);
	}
	if (args.seed !== undefined) meta.push(`seed=${args.seed}`);
	if (args.reference_image && args.reference_image.trim().length > 0) {
		meta.push("📎 reference attached");
	}
	const headerLine = `${theme.fg("toolTitle", theme.bold("OpenRouter Image"))} ${theme.fg("muted", meta.join(" "))}`;
	const promptLine = theme.fg("accent", prompt);
	return wrapInBox(new Text(`${headerLine}\n${promptLine}`, 0, 0), theme, "muted");
}
export function renderResult(
  result: AgentToolResult<OpenrouterImageDetails>,
  _options: ToolRenderResultOptions,
  theme: Theme,
  args?: OpenrouterImageParams,
): Component {
	const liveStatus = result.details?.liveStatus;
	const elapsed = result.details?.elapsedSeconds;
	const inFlight = !result.isError && (liveStatus === "generating" || liveStatus === "queued");

	if (result.isError) {
		const msg = result.content.map((c) => ("text" in c ? c.text : "")).join("\n");
		const headerParts = [theme.fg("error", theme.bold("✗ OpenRouter Image — error"))];
		return wrapInBox(new Text([...headerParts, msg].join(" "), 0, 0), theme, "error");
	}

	const paths = result.details?.savedPaths ?? [];
	const model = result.details?.model ?? args?.model ?? DEFAULT_MODEL;
	const mediaType = result.details?.mediaType ?? "image/png";
	const usage = result.details?.usage;
	const cost = result.details?.cost ?? usage?.cost;
	const prompt = stripControls(args?.prompt ?? "");
	const reference = args?.reference_image && args.reference_image.trim().length > 0 ? " 📎" : "";

	if (inFlight) {
		const headerLine = `${theme.fg("toolTitle", theme.bold("OpenRouter Image"))} ${theme.fg("muted", `model=${model}`)}${reference} ${theme.fg("accent", `⏳ ${liveStatus}${elapsed !== undefined ? ` — ${elapsed}s` : ""}`)}`;
		const promptLine = theme.fg("accent", prompt);
		return wrapInBox(new Text(`${headerLine}\n${promptLine}`, 0, 0), theme, "accent");
	}

	const lines: string[] = [];
	const headerParts = [theme.fg("toolTitle", theme.bold("OpenRouter Image")), model, `${paths.length} image(s)`, mediaType];
	if (elapsed !== undefined) headerParts.push(`took ${elapsed}s`);
	if (liveStatus) headerParts.push(`(${liveStatus})`);
	lines.push(headerParts.join(" — "));
	const statParts: string[] = [];
	if (result.details?.b64Length) statParts.push(`b64=${result.details.b64Length}`);
	if (cost !== undefined) statParts.push(`cost=$${cost.toFixed(6)}`);
	if (usage?.total_tokens !== undefined) statParts.push(`tokens=${usage.total_tokens}`);
	if (statParts.length > 0) lines.push(`   ${statParts.join(" · ")}`);

	const textBlock = result.content.find((c) => c.type === "text");
	if (textBlock && "text" in textBlock) {
		lines.push("");
		lines.push(textBlock.text);
	}
	return wrapInBox(new Text(lines.join("\n"), 0, 0), theme, "success");
}

export default function openrouterImageExtension(pi: ExtensionAPI): void {
	const { Type } = pi.typebox;

	const OpenrouterImageParamsSchema = Type.Object({
		prompt: Type.String({ description: "Image generation prompt" }),
		model: Type.Optional(
			Type.String({
				description: `Model slug (default ${DEFAULT_MODEL}). Options: openai/gpt-image-2, openai/gpt-image-1, openai/gpt-image-1-mini, openai/gpt-5-image, openai/gpt-5-image-mini, openai/gpt-5.4-image-2`,
			}),
		),
		reference_image: Type.Optional(
			Type.String({
				description: "Absolute or workspace-relative path to a local PNG/JPG/JPEG/GIF/WEBP file. Read, base64-encoded, and sent as visual context to the model.",
			}),
		),
		aspect_ratio: Type.Optional(
			Type.String({
				description: `Aspect ratio (default ${DEFAULT_ASPECT_RATIO}): 1:1, 3:2, 2:3, 4:3, 3:4, 16:9, 9:16, 21:9, auto`,
			}),
		),
		quality: Type.Optional(
			Type.String({ description: "Quality preset: auto, low, medium, high" }),
		),
		background: Type.Optional(
			Type.String({ description: "Background mode: auto, transparent, opaque" }),
		),
		output_format: Type.Optional(
			Type.String({
				description: `Output format (default ${DEFAULT_OUTPUT_FORMAT}): png, jpeg, webp, svg`,
			}),
		),
		resolution: Type.Optional(
			Type.String({ description: "Resolution: 512, 1K, 2K, 4K" }),
		),
		n: Type.Optional(
			Type.Number({ description: "Number of images to generate (1-10)", minimum: 1, maximum: 10 }),
		),
		seed: Type.Optional(Type.Number({ description: "Random seed for reproducibility" })),
	});

	pi.registerTool({
		name: "openrouter_image",
		label: "OpenRouter Image Generation",
		description:
			"Generate images via OpenRouter's GPT Image models (openai/gpt-image-1, gpt-image-2, gpt-5-image, etc). Optionally accepts a reference image (data URL, https URL, or local file path) for visual context. Decodes base64 responses and writes PNG/JPEG/WEBP/SVG files to ~/.omp/agent/generated-images/.",
		parameters: OpenrouterImageParamsSchema,
		hidden: false,
		renderCall,
		renderResult,
		execute: async (_toolCallId, rawParams, signal, onUpdate, _ctx) => {
			return executeOpenrouterImage(
				rawParams as OpenrouterImageParams,
				onUpdate as AgentToolUpdateCallback<OpenrouterImageDetails> | undefined,
				signal,
			);
		},
	});
}