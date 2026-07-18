/**
 * Stage tabs, live tab colors, auto-jump/scroll/highlight, and "Other" toggle.
 */

(function () {
  const lastCount = {};

  function initTabs() {
    const panelsRoot = document.getElementById('stage-panels');
    if (!panelsRoot) return;

    const activeSlug = panelsRoot.getAttribute('data-active');
    document.querySelectorAll('.stage-panel').forEach(function (panel) {
      panel.classList.toggle('active', panel.getAttribute('data-slug') === activeSlug);
    });
    document.querySelectorAll('#stage-tabs .tab').forEach(function (tab) {
      tab.classList.toggle('selected', tab.getAttribute('data-slug') === activeSlug);
    });

    document.getElementById('stage-tabs')?.addEventListener('click', function (evt) {
      const tab = evt.target.closest('.tab');
      if (!tab || tab.disabled) return;
      const slug = tab.getAttribute('data-slug');
      const target = tab.getAttribute('data-target');
      if (!slug || !target) return;

      document.querySelectorAll('.stage-panel').forEach(function (p) {
        p.classList.remove('active');
      });
      document.querySelectorAll('#stage-tabs .tab').forEach(function (t) {
        t.classList.remove('selected');
      });

      const panel = document.querySelector(target);
      if (panel) panel.classList.add('active');
      tab.classList.add('selected');
    });
  }

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

  function onAfterSwap(evt) {
    const editor = evt.target.querySelector?.('textarea[name="content"]:not([readonly])');
    if (editor) editor.focus();

    refreshTabColors();

    const wrapper = evt.target.closest?.('[data-stage-status]') ||
      (evt.target.matches?.('[data-stage-status]') ? evt.target : null);
    if (!wrapper) return;

    const panel = wrapper.closest('.stage-panel');
    if (!panel) return;

    const slug = wrapper.getAttribute('data-stage-slug');
    const status = wrapper.getAttribute('data-stage-status');
    const count = parseInt(wrapper.getAttribute('data-msg-count') || '0', 10);
    const prev = lastCount[slug] || 0;
    lastCount[slug] = count;

    if ((status === 'running' || status === 'awaiting') && count > prev) {
      const isActive = panel.classList.contains('active');
      if (!isActive && slug) {
        const tab = document.querySelector('#stage-tabs .tab[data-slug="' + slug + '"]');
        if (tab && !tab.disabled) tab.click();
      }
      const msgs = wrapper.querySelectorAll('.stage-msg');
      const last = msgs[msgs.length - 1];
      if (last) {
        last.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        last.classList.add('msg-highlight');
        setTimeout(function () {
          last.classList.remove('msg-highlight');
        }, 2000);
      }
    }

    if (status === 'done' && slug === 'plan_review') {
      const implTab = document.querySelector('#stage-tabs .tab[data-slug="implementation"]');
      if (implTab) {
        implTab.disabled = false;
        implTab.classList.remove('tab--disabled');
        implTab.click();
      }
    }
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

  function seedCounts() {
    document.querySelectorAll('[data-stage-slug][data-msg-count]').forEach(function (w) {
      lastCount[w.getAttribute('data-stage-slug')] =
        parseInt(w.getAttribute('data-msg-count') || '0', 10);
    });
  }

  document.addEventListener('DOMContentLoaded', function () {
    initTabs();
    seedCounts();
    refreshTabColors();
    initOtherToggle();
  });

  document.body.addEventListener('htmx:afterSwap', onAfterSwap);

  document.body.addEventListener('htmx:responseError', function (evt) {
    console.error('htmx error:', evt.detail);
  });
})();
