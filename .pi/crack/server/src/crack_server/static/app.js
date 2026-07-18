/**
 * Task page behaviour. Tabs are now real links (each tab is its own page), and
 * the server force-navigates the user forward via the auto-follow poller's
 * HX-Redirect — so there is no client-side tab switching here anymore. What's
 * left: live tab colours, scroll-to-latest on new content, and the Q&A "Other"
 * toggle.
 */

(function () {
  function refreshTabColors() {
    document.querySelectorAll('[data-stage-status]').forEach(function (wrapper) {
      const slug = wrapper.getAttribute('data-stage-slug');
      const status = wrapper.getAttribute('data-stage-status');
      if (!slug) return;
      const tab = document.querySelector('#stage-tabs .tab[data-slug="' + slug + '"]');
      if (!tab) return;
      tab.classList.remove('tab--running', 'tab--done', 'tab--idle', 'tab--disabled', 'tab--error');
      const cls = {
        running: 'tab--running',
        awaiting: 'tab--running',
        done: 'tab--done',
        idle: 'tab--idle',
        disabled: 'tab--disabled',
        error: 'tab--error',
      }[status] || 'tab--idle';
      tab.classList.add(cls);
    });
  }

  function scrollToLatest(evt) {
    const wrapper = evt.target.closest?.('[data-stage-status]') ||
      (evt.target.matches?.('[data-stage-status]') ? evt.target : null);
    if (!wrapper) return;
    // Keep the newest item (a fresh turn, or the bottom spinner) in view.
    const msgs = wrapper.querySelectorAll('.stage-msg');
    const last = msgs[msgs.length - 1];
    if (last) last.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  function onAfterSwap(evt) {
    const editor = evt.target.querySelector?.('textarea[name="content"]:not([readonly])');
    if (editor) editor.focus();
    refreshTabColors();
    scrollToLatest(evt);
  }

  function initOtherToggle() {
    document.body.addEventListener('change', function (evt) {
      const input = evt.target;
      if (!input.matches('input[type="radio"], input[type="checkbox"]')) return;
      const fieldset = input.closest('.plan-question');
      if (!fieldset) return;

      const name = input.name;
      const otherTa = fieldset.querySelector('textarea[name="' + name + '__other"]');
      if (!otherTa) return;

      if (input.type === 'radio') {
        const isOther = input.value === '__other__' && input.checked;
        otherTa.disabled = !isOther;
        otherTa.classList.toggle('show', isOther);
        if (!isOther && !fieldset.querySelector('input[value="__other__"]:checked')) {
          otherTa.classList.remove('show');
        }
      } else if (input.type === 'checkbox' && input.value === '__other__') {
        otherTa.disabled = !input.checked;
        otherTa.classList.toggle('show', input.checked);
      }
    });
  }

  document.addEventListener('DOMContentLoaded', function () {
    refreshTabColors();
    initOtherToggle();
  });

  document.body.addEventListener('htmx:afterSwap', onAfterSwap);

  document.body.addEventListener('htmx:responseError', function (evt) {
    console.error('htmx error:', evt.detail);
  });
})();
