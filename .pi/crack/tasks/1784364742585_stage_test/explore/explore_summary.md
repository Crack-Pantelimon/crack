**Overview**

The task page is rendered by the `task_page` view in **`src/crack_server/app.py`** (lines 451‑482).  
This function builds the full HTML response by calling `_render_base`, which injects the page header, prompt list, stage tabs, and any other fragments. The header itself is produced by `_render_task_header` and ultimately by `_render_title_h1`/`_render_title_input`, so any footer that should appear on every task page would need to be added inside the `body` string that `task_page` returns.

Static assets are served from the **`static/`** directory under the same package.  
The server explicitly mounts this folder, and the two files that are shipped with the UI are:

- `app.css` – the stylesheet used by the task page and all other UI components.  
- `app.js` – the minimal JavaScript that powers htmx interactions.

Because the task page’s HTML is assembled in `task_page` and the footer would be part of the `body` fragment returned by `_render_base`, the correct place to inject a small server‑name note is right before the closing `</section>` or at the bottom of the `body` string, ensuring it appears after the main content but before the final `"""` that closes the template.

**Key take‑aways**

- The server‑side rendering entry point is `task_page` in **`src/crack_server/app.py`**.  
- All dynamic fragments (including the prompt list and stage tabs) are rendered via helper functions that feed into `_render_base`.  
- Static assets live in **`src/crack_server/static/`**, namely `app.css` and `app.js`.  

---

### Specific file references

- `src/crack_server/app.py:451-482`  
- `src/crack_server/app.py:130-190`  
- `src/crack_server/static/app.css:1-24`  
- `src/crack_server/static/app.js:1-10`