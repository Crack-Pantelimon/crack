Q: Where is the explore button rendered in the UI, and what template or component controls its visibility and disabled state?
A: The explore button is likely rendered in a Jinja2 template or HTMX partial in `.pi/crack/server/src/crack_server/ui.py` or a templates directory, with conditional logic checking for prompt files existence.

Q: How does the server determine if prompt files exist? Is there a function that scans for prompt files or checks a prompt directory?
A: There's likely a utility function in `crack_server` that scans a prompts directory (e.g., `prompts/` or `.pi/prompts/`) and returns a count or list of prompt files, used by the explore endpoint.

Q: Which endpoint or route serves the page containing the explore button, and does it pass prompt file availability to the template context?
A: The main dashboard or chat route (likely `/` or `/chat`) in the server's route handlers passes a `has_prompts` boolean or `prompt_count` to the template context.

Q: Is there an HTMX endpoint that re-renders just the explore button when prompt files change, or does it require a full page reload?
A: There may be an HTMX partial endpoint like `/partial/explore-button` that returns the button fragment with correct disabled state, triggered by a poll or after prompt creation.

Q: Where are prompt files created/managed, and is there an event or signal that could trigger the explore button to re-evaluate its state?
A: Prompt files are likely managed via a `/prompts` API endpoint (POST to create), and after creation, the UI either polls or uses HTMX to refresh the explore button fragment.

Q: Does the existing codebase have a pattern for conditionally disabling/enabling buttons based on server-side state (e.g., other buttons that follow this pattern)?
A: Other buttons like "Save" or "Run" likely follow a similar pattern — checking a condition in the template and rendering `disabled` attribute or hiding via `hx-show`/`hx-hide`.

Q: Is there a JavaScript module or Alpine.js component that handles dynamic UI state for the explore button, or is it purely server-rendered?
A: The project uses HTMX with minimal JS, so the explore button state is likely purely server-rendered via template conditionals, not client-side JS.

Q: What is the exact CSS class or styling used for disabled buttons in this codebase, to ensure consistency?
A: Disabled buttons likely use a standard class like `btn-disabled`, `opacity-50`, or Tailwind's `disabled:opacity-50 disabled:cursor-not-allowed` utility classes.