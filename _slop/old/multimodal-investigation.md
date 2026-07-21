# Pi Coding Agent - Multimodal/Image Input Support Investigation

## Summary

**Yes, Pi (the coding agent) supports multimodal model turns including image input**, but the support depends on the model being used and whether you have vision-capable extensions/packages installed.

---

## Native Vision Support (Built-in)

### 1. Model-Level Vision Capability
Pi's model configuration supports vision through the `input` field in `~/.pi/agent/models.json`:

```json
{
  "providers": {
    "openai": {
      "models": [
        { "id": "gpt-4o", "input": ["text", "image"] }
      ]
    }
  }
}
```

Models with `input: ["text", "image"]` can natively receive images in the conversation.

### 2. Built-in `read` Tool Supports Images
From the Pi README (pi-coding-agent/README.md):
> **read**: Read the contents of a file. Supports text files and **images (jpg, png, gif, webp)**. Images are sent as attachments.

You can:
- Paste an image into the Pi TUI (Ctrl+V) - Pi saves it to a temp file and inserts the path
- Use `@image.png` in your prompt to include an image
- Use the `read` tool on an image file path

### 3. Native Vision Models (Work Out of the Box)
Models that natively support image input (configured with `input: ["text", "image"]`):
- **OpenAI**: gpt-4o, gpt-4o-mini, gpt-4.1-mini, gpt-5
- **Anthropic**: claude-sonnet-4, claude-3.5-haiku, claude-opus-4
- **Google**: gemini-2.5-flash, gemini-2.5-pro, gemini-2.0-flash
- **xAI**: grok-2, grok-2-vision
- **Moonshot/Kimi**: kimi-k2.5
- **Local models (llama.cpp/Ollama)**: Qwen2.5-VL, Gemma 3, LLaVA, etc. (when configured with `input: ["text", "image"]`)

---

## Vision Support for Text-Only Models (via Extensions/Packages)

If your primary coding model lacks vision (e.g., DeepSeek V4, DeepSeek V3, local text-only models), you can add vision capability via Pi packages:

### 1. `pi-vision-proxy` (from DeepakNess blog, pi.dev/packages)
- **Install**: `pi install npm:pi-vision-proxy`
- **How it works**: Intercepts images before the LLM call → sends to a configured vision model (e.g., Kimi K2.6) → replaces image with text description → text-only model receives description
- **Features**: Batching multiple images, perceptual hash caching, grounding formats for coordinate targeting
- **Config**: Set vision model via `/visionizer-model` command

### 2. `pi-visionizer` (GitHub: Oyaxira/pi-visionizer)
- **Install**: `pi install ./pi-visionizer` (local) or `pi install npm:pi-visionizer`
- **How it works**: Transparent proxy - intercepts images, sends to vision model, replaces with `[ Description: … ]`
- **Commands**: `/visionizer-model`, `/visionizer-prompt`, `/visionizer-status`, `/visionizer-clear`
- **Features**: Caching, graceful failure, zero impact when disabled, auto-removes `describe_image` tool for native vision models

### 3. `pi-image-subagent` (GitHub: AlvaroRausell/pi-image-subagent)
- **Install**: Manual extension install (`~/.pi/agent/extensions/analyze-image/`)
- **How it works**: Adds `analyze_image` tool that spawns an isolated Pi subagent with a vision model (default: gemma4:31b-cloud)
- **Features**: Subagent has only `read` tool, stateless, batch multiple images, configurable model/prompt
- **Use case**: "What's in this screenshot?" → agent calls `analyze_image` → gets text description

### 4. `pi-multimodal-proxy` (Official pi.dev package)
- **Install**: `pi install npm:pi-multimodal-proxy`
- **Package**: `@ngsoftware/pi-multimodal-proxy` (v1.10.1)
- **Extension**: `./extensions/vision-proxy.ts`
- **Author**: ngsoftware (MIT license)

---

## Configuration Examples

### Native Vision Model (Gemini via Google AI Studio)
```json
{
  "providers": {
    "my-google": {
      "baseUrl": "https://generativelanguage.googleapis.com/v1beta",
      "api": "google-generative-ai",
      "apiKey": "$GEMINI_API_KEY",
      "models": [
        { 
          "id": "gemini-2.5-flash", 
          "input": ["text", "image"],
          "contextWindow": 1048576
        }
      ]
    }
  }
}
```

### Local Vision Model (llama.cpp / Ollama)
```json
{
  "providers": {
    "llama-cpp": {
      "baseUrl": "http://localhost:8080/v1",
      "api": "openai-completions",
      "apiKey": "none",
      "models": [
        { "id": "qwen2.5-vl-7b", "input": ["text", "image"] }
      ]
    }
  }
}
```

### Using pi-visionizer with DeepSeek + GPT-4o Vision
1. Configure DeepSeek as default model (text-only)
2. Configure OpenAI GPT-4o as vision model
3. Run `/visionizer-model` and select `openai/gpt-4o-mini`
4. Paste screenshot → DeepSeek receives text description

---

## How Image Input Works in Pi TUI

| Method | Description |
|--------|-------------|
| **Ctrl+V** | Paste image from clipboard → saved to temp file → path inserted in editor |
| **Drag & drop** | Drag image onto terminal → same as paste |
| **`@path/to/image.png`** | Reference local image file in prompt |
| **`read` tool** | Model calls `read` tool on image path → image sent as attachment |

---

## Key Documentation Sources

| Source | URL | Key Info |
|--------|-----|----------|
| Pi README (models) | https://github.com/earendil-works/pi/blob/main/packages/coding-agent/README.md | `read` tool supports images, model `input` field |
| Pi models.md | https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md | Model config with `input: ["text", "image"]` |
| DeepakNess blog | https://deepakness.com/blog/pi-agent-setup/ | pi-vision-proxy usage with DeepSeek + Kimi |
| pi-visionizer | https://github.com/Oyaxira/pi-visionizer | Transparent vision proxy for text-only models |
| pi-image-subagent | https://github.com/AlvaroRausell/pi-image-subagent | Subagent-based image analysis tool |
| pi-multimodal-proxy | https://pi.dev/packages/pi-multimodal-proxy | Official vision proxy package |
| Hugging Face Pi guide | https://huggingface.co/docs/hub/agents-local | Local vision models with `input: ["text", "image"]` |

---

## Summary Table

| Scenario | Solution |
|----------|----------|
| Using GPT-4o, Claude, Gemini, Kimi | **Native** - works out of the box with `read` tool / paste |
| Using local Qwen2.5-VL, Gemma 3, LLaVA | **Native** - add `input: ["text", "image"]` to models.json |
| Using DeepSeek V4/V3 (text-only) | **pi-vision-proxy** or **pi-visionizer** + vision model |
| Using local text-only Ollama model | **pi-visionizer** or **pi-image-subagent** + vision model |
| Need precise image analysis (coordinates, UI) | **pi-vision-proxy** (grounding formats) or **pi-image-subagent** |
| Want zero-config, just works | Use a native vision model (GPT-4o, Gemini, Claude) |

---

## Conclusion

Pi **does support multimodal turns** natively for vision-capable models. For text-only models (like DeepSeek), there are **three mature community packages** that transparently add vision capability by proxying images to a vision model. The ecosystem is well-documented and the packages are actively maintained (pi-vision-proxy, pi-visionizer, pi-image-subagent, pi-multimodal-proxy).