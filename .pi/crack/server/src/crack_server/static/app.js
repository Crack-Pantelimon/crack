/**
 * Minimal JS for htmx interactions
 * Most functionality is handled by htmx attributes in HTML
 */

(function () {
  // Auto-focus the content textarea when a prompt row switches to edit mode
  document.body.addEventListener('htmx:afterSwap', function (evt) {
    const editor = evt.target.querySelector('textarea[name="content"]:not([readonly])');
    if (editor) {
      editor.focus();
    }
  });

  // Global htmx error handling
  document.body.addEventListener('htmx:responseError', function (evt) {
    console.error('htmx error:', evt.detail);
  });
})();
