---
name: take-screenshot-using-browser
description: Capture a screenshot of a website with a specific resolution and save it to a file path, then verify the file exists and has the expected dimensions.
---

# Skill: Take Screenshot Using Browser


## When to use

Use this skill when you need to save a screenshot of a webpage to a specific file path. The output will be a PNG image with the requested viewport dimensions.

## Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `url` | The website URL to capture | required |
| `output_path` | Absolute path where the PNG should be saved | required |
| `width` | Screenshot width in pixels | 1920 |
| `height` | Screenshot height in pixels | 1080 |
| `wait_ms` | Optional wait time after navigation before capturing | 2000 |

## Steps

1. **Open a new browser tab and navigate to the URL.**

   Use the `chromium_new_page` tool with the `url` parameter.

   ```json
   {
     "url": "https://github.com"
   }
   ```

2. **Resize the browser viewport to the target resolution.**

   Use the `chromium_resize_page` tool with `width` and `height`.

   ```json
   {
     "width": 1920,
     "height": 1080
   }
   ```

3. **Wait for the page to finish loading (optional but recommended).**

   Use the `chromium_evaluate_script` tool to wait a short time.

   ```json
   {
     "function": "() => new Promise(r => setTimeout(r, 2000))"
   }
   ```

4. **Capture the screenshot and save it to the requested path.**

   Use the `chromium_take_screenshot` tool with the `filePath` parameter.

   ```json
   {
     "filePath": "/workspace/_test_gh.png"
   }
   ```

   The tool will confirm the file was saved. Do not rely on the returned image preview; the file is written to disk.

5. **Verify the screenshot exists and has the correct dimensions.**

   Use `magick` via the Bash tool:

   ```bash
   magick identify -format "%w %h" /workspace/_test_gh.png
   ```

   Or use `ffprobe` via the Bash tool:

   ```bash
   ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=s=x:p=0 /workspace/_test_gh.png
   ```

   Both commands should output the dimensions, for example `1920x1080`.

6. **Report the result.**

   State the output path, file size, and dimensions.

## Important notes

- Always use `filePath` with `chromium_take_screenshot`; otherwise the screenshot is returned as inline data and may not be saved to disk.
- If `chromium_new_page` fails with a parameter error, check that the JSON key is exactly `url`.
- If the page content is dynamic, increase `wait_ms` or wait for a specific element before capturing.
- Prefer `magick` over `identify`; modern ImageMagick installs the command as `magick`.
- If neither `magick` nor `ffprobe` is available, install them or use `file` to at least confirm the file exists and is a PNG.

## Example full run

```json
// chromium_new_page
{"url": "https://github.com"}

// chromium_resize_page
{"width": 1920, "height": 1080}

// chromium_evaluate_script
{"function": "() => new Promise(r => setTimeout(r, 2000))"}

// chromium_take_screenshot
{"filePath": "/workspace/_test_gh.png"}

// bash
magick identify -format "%w %h" /workspace/_test_gh.png
```
