/**
 * Chat page behaviour: long-poll watch loop, incremental msg append,
 * scroll-on-new-content (near bottom only), details open-state restore,
 * Q&A "Other" toggle, and sidebar/home chat status dots.
 */

(function () {
  function sleep(ms) {
    return new Promise(function (resolve) {
      setTimeout(resolve, ms);
    });
  }

  function applyStatusMeta(slug) {
    const meta = document.getElementById(slug + '-status-meta');
    const content = document.getElementById(slug + '-content');
    if (!meta || !content) return;
    ['data-stage-status', 'data-msg-count', 'data-state-mtime'].forEach(function (attr) {
      const v = meta.getAttribute(attr);
      if (v != null) content.setAttribute(attr, v);
    });
  }

  function lastMsgIndex(slug) {
    const msgs = document.getElementById(slug + '-msgs');
    if (!msgs) return -1;
    let max = -1;
    const re = new RegExp('^' + slug.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '-msg-(\\d+)$');
    msgs.querySelectorAll(':scope > [id]').forEach(function (el) {
      const m = el.id.match(re);
      if (m) max = Math.max(max, parseInt(m[1], 10));
    });
    return max;
  }

  function nearBottom(el, threshold) {
    if (!el) return true;
    return el.scrollHeight - el.scrollTop - el.clientHeight < (threshold || 200);
  }

  function scrollIfNearBottom(slug, prevCount) {
    const content = document.getElementById(slug + '-content');
    if (!content) return;
    const count = parseInt(content.getAttribute('data-msg-count') || '0', 10);
    if (!(count > prevCount)) return;
    const scrollRoot = document.scrollingElement || document.documentElement;
    if (!nearBottom(scrollRoot, 200)) return;
    const msgs = document.getElementById(slug + '-msgs');
    const last = msgs && msgs.lastElementChild;
    if (last) last.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  const openDetails = Object.create(null);

  function rememberDetails(root) {
    if (!root) return;
    root.querySelectorAll('details[id], .stage-msg details, details.stage-msg').forEach(function (d) {
      const key = d.id || (d.closest('[id]') && d.closest('[id]').id + ':' +
        Array.prototype.indexOf.call(d.parentNode.children, d));
      if (key) openDetails[key] = d.open;
    });
  }

  function restoreDetails(root) {
    if (!root) return;
    root.querySelectorAll('details[id], .stage-msg details, details.stage-msg').forEach(function (d) {
      const key = d.id || (d.closest('[id]') && d.closest('[id]').id + ':' +
        Array.prototype.indexOf.call(d.parentNode.children, d));
      // Apply the exact remembered state so a user can collapse a still-running
      // sub-agent card without the 2s self-poll re-expanding it.
      if (key && key in openDetails) d.open = openDetails[key];
    });
  }

  document.body.addEventListener('toggle', function (evt) {
    const d = evt.target;
    if (!(d instanceof HTMLDetailsElement)) return;
    const key = d.id || (d.closest('[id]') && d.closest('[id]').id + ':' +
      Array.prototype.indexOf.call(d.parentNode.children, d));
    if (key) openDetails[key] = d.open;
  }, true);

  function lastChatMsgIndex() {
    return lastMsgIndex('chat');
  }

  async function fetchChatDelta(chatId) {
    const content = document.getElementById('chat-content');
    const prevCount = parseInt((content && content.getAttribute('data-msg-count')) || '0', 10);
    const after = lastChatMsgIndex();
    rememberDetails(document.getElementById('chat-msgs'));
    await htmx.ajax('GET', '/chats/' + encodeURIComponent(chatId) + '/status?after=' + after, {
      target: '#chat-msgs',
      swap: 'beforeend',
    });
    applyStatusMeta('chat');
    restoreDetails(document.getElementById('chat-msgs'));
    restoreDetails(document.getElementById('chat-tail'));
    scrollIfNearBottom('chat', prevCount);
  }

  async function watchChat(chatId) {
    const content = document.getElementById('chat-content');
    let since = (content && content.dataset.stateMtime) || '0';
    for (;;) {
      try {
        const r = await fetch(
          '/chats/' + encodeURIComponent(chatId) +
          '/wait?since=' + encodeURIComponent(since)
        );
        if (!r.ok) {
          await sleep(2000);
          continue;
        }
        const j = await r.json().catch(function () { return null; });
        if (!j) {
          await sleep(2000);
          continue;
        }
        since = j.since;
        if (j.changed) {
          await fetchChatDelta(chatId);
        }
      } catch (e) {
        await sleep(2000);
      }
    }
  }

  function applyDot(el, status) {
    if (!el || !status) return;
    const phase = status.phase || 'idle';
    const tool = status.tool || 'none';
    el.className = 'chat-dot dot-' + phase;
    el.setAttribute('title', phase + ' / tool:' + tool);
    let inner = el.querySelector('.chat-dot-inner');
    if (!inner) {
      inner = document.createElement('span');
      inner.className = 'chat-dot-inner';
      el.appendChild(inner);
    }
    inner.className = 'chat-dot-inner tool-' + tool;
  }

  function applyDots(dots) {
    if (!dots) return;
    Object.keys(dots).forEach(function (cid) {
      document.querySelectorAll('[data-chat-id="' + cid + '"].chat-dot').forEach(function (el) {
        applyDot(el, dots[cid]);
      });
    });
  }

  async function watchDots() {
    let since = '0';
    for (;;) {
      try {
        const r = await fetch(
          '/api/chats/dots/wait?since=' + encodeURIComponent(since)
        );
        if (!r.ok) {
          await sleep(2000);
          continue;
        }
        const j = await r.json().catch(function () { return null; });
        if (!j) {
          await sleep(2000);
          continue;
        }
        since = j.since;
        applyDots(j.dots);
      } catch (e) {
        await sleep(2000);
      }
    }
  }

  function onAfterSwap(evt) {
    const editor = evt.target.querySelector?.('textarea[name="content"]:not([readonly])');
    if (editor) editor.focus();

    const content = evt.target.closest?.('[data-stage-status]') ||
      (evt.target.matches?.('[data-stage-status]') ? evt.target : null);
    if (content) {
      const slug = content.getAttribute('data-stage-slug');
      if (slug) applyStatusMeta(slug);
    }

    // Self-polling sub-agent regions swap their own outerHTML every 2s; restore
    // any user collapse/expand state on the freshly swapped-in details.
    if (evt.target.querySelector?.('details') || evt.target.matches?.('details')) {
      restoreDetails(evt.target);
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

  function applyPlanToggle(form) {
    if (!form) return;
    const on = !!form.querySelector('[data-plan-toggle]:checked');
    const planFields = form.querySelector('[data-plan-fields]');
    const nonplanFields = form.querySelector('[data-nonplan-fields]');
    if (planFields) planFields.hidden = !on;
    if (nonplanFields) nonplanFields.hidden = on;
  }

  function initPlanToggle() {
    document.body.addEventListener('change', function (evt) {
      const input = evt.target;
      if (!input.matches || !input.matches('[data-plan-toggle]')) return;
      applyPlanToggle(input.closest('[data-plan-form]'));
    });
    document.querySelectorAll('[data-plan-form]').forEach(applyPlanToggle);
  }

  document.addEventListener('DOMContentLoaded', function () {
    initOtherToggle();
    initPlanToggle();
    watchDots();

    const chat = document.getElementById('chat-content');
    if (chat && chat.dataset.chatId) {
      watchChat(chat.dataset.chatId);
    }
  });

  document.body.addEventListener('htmx:afterSwap', onAfterSwap);

  document.body.addEventListener('htmx:responseError', function (evt) {
    console.error('htmx error:', evt.detail);
  });

  document.body.addEventListener('click', function (evt) {
    const collapseBtn = evt.target.closest && evt.target.closest('.md-collapse-btn');
    if (collapseBtn) {
      const details = collapseBtn.closest('details');
      if (details) details.open = false;
      evt.preventDefault();
      return;
    }
    const img = evt.target.closest && evt.target.closest('img.tool-thumb');
    if (!img) return;
    let dlg = document.getElementById('img-lightbox');
    if (!dlg) {
      dlg = document.createElement('dialog');
      dlg.id = 'img-lightbox';
      dlg.innerHTML = '<img alt="expanded image">';
      document.body.appendChild(dlg);
      dlg.addEventListener('click', function () { dlg.close(); });
    }
    dlg.querySelector('img').src = img.getAttribute('src');
    dlg.showModal();
  });

  function attachmentEndpointFor(ta) {
    if (ta.name === 'msg') {
      const form = ta.closest('form');
      const hx = (form && form.getAttribute('hx-post')) || '';
      const m = hx.match(/^\/api\/chats\/([^/]+)\/messages/);
      if (m) {
        return {
          url: '/api/chats/' + encodeURIComponent(m[1]) + '/attachments',
          strip: 'chat-attachments',
        };
      }
    }
    return null;
  }

  function imageFilesFrom(dt) {
    if (!dt) return [];
    const files = [];
    if (dt.items) {
      for (const item of dt.items) {
        if (item.kind === 'file' && item.type.startsWith('image/')) {
          const f = item.getAsFile();
          if (f) files.push(f);
        }
      }
    } else if (dt.files) {
      for (const f of dt.files) {
        if (f.type.startsWith('image/')) files.push(f);
      }
    }
    return files;
  }

  function uploadAttachment(ep, file) {
    const fd = new FormData();
    fd.append('file', file, file.name || 'pasted-image.png');
    const strip = document.getElementById(ep.strip);
    let placeholder = null;
    if (strip) {
      placeholder = document.createElement('span');
      placeholder.className = 'attachment-chip loading';
      placeholder.setAttribute('aria-busy', 'true');
      placeholder.setAttribute('title', 'Uploading image…');
      strip.appendChild(placeholder);
    }
    fetch(ep.url, { method: 'POST', body: fd })
      .then(function (r) {
        if (!r.ok) return r.text().then(function (t) { throw new Error(t); });
        return r.text();
      })
      .then(function (html) {
        if (placeholder) {
          const tpl = document.createElement('template');
          tpl.innerHTML = html.trim();
          placeholder.replaceWith(tpl.content);
        } else if (strip) {
          strip.insertAdjacentHTML('beforeend', html);
        }
      })
      .catch(function (e) {
        if (placeholder) placeholder.remove();
        console.error('attachment upload failed:', e);
        alert('Image upload failed: ' + e.message);
      });
  }

  document.body.addEventListener('paste', function (evt) {
    const ta = evt.target.closest && evt.target.closest('textarea');
    if (!ta) return;
    const ep = attachmentEndpointFor(ta);
    if (!ep) return;
    const files = imageFilesFrom(evt.clipboardData);
    if (!files.length) return;
    evt.preventDefault();
    files.forEach(function (f) { uploadAttachment(ep, f); });
  });

  document.body.addEventListener('dragover', function (evt) {
    const ta = evt.target.closest && evt.target.closest('textarea');
    if (!ta || !attachmentEndpointFor(ta)) return;
    evt.preventDefault();
  });

  document.body.addEventListener('drop', function (evt) {
    const ta = evt.target.closest && evt.target.closest('textarea');
    if (!ta) return;
    const ep = attachmentEndpointFor(ta);
    if (!ep) return;
    const files = imageFilesFrom(evt.dataTransfer);
    if (!files.length) return;
    evt.preventDefault();
    files.forEach(function (f) { uploadAttachment(ep, f); });
  });

  // ---- Patch review (diff2html) -------------------------------------------
  function initPatchReviews(root) {
    const scope = root || document;
    if (typeof Diff2HtmlUI === 'undefined' && typeof Diff2Html === 'undefined') return;
    scope.querySelectorAll('.patch-review').forEach(function (panel) {
      if (panel.getAttribute('data-d2h-ready') === '1') return;
      const diffEl = panel.querySelector('script.patch-review-diff');
      const target = panel.querySelector('.patch-review-d2h');
      if (!diffEl || !target) return;
      const diffText = diffEl.textContent || '';
      if (!diffText.trim()) {
        target.innerHTML = '<p class="muted">Empty diff.</p>';
        panel.setAttribute('data-d2h-ready', '1');
        return;
      }
      let outputFormat = 'line-by-line';
      function render() {
        const cfg = {
          drawFileList: true,
          matching: 'lines',
          outputFormat: outputFormat,
          synchronisedScroll: true,
          highlight: true,
          fileListToggle: true,
          fileListStartVisible: true,
        };
        try {
          if (typeof Diff2HtmlUI !== 'undefined') {
            const ui = new Diff2HtmlUI(target, diffText, cfg);
            ui.draw();
          } else {
            target.innerHTML = Diff2Html.getPrettyHtml(diffText, cfg);
          }
        } catch (e) {
          console.error('diff2html failed', e);
          target.innerHTML = '<pre class="traj-note-log"></pre>';
          target.querySelector('pre').textContent = diffText.slice(0, 20000);
        }
        wireCommentGutters(panel, target);
      }
      render();
      panel.setAttribute('data-d2h-ready', '1');
      panel.querySelectorAll('.patch-view-toggle').forEach(function (btn) {
        btn.addEventListener('click', function () {
          outputFormat = btn.getAttribute('data-view') || 'line-by-line';
          panel.removeAttribute('data-d2h-ready');
          target.innerHTML = '';
          panel.setAttribute('data-d2h-ready', '0');
          // re-init
          const was = panel.getAttribute('data-d2h-ready');
          panel.setAttribute('data-d2h-ready', '0');
          render();
          panel.setAttribute('data-d2h-ready', '1');
        });
      });
      const bodyToggle = panel.querySelector('.patch-body-toggle');
      if (bodyToggle) {
        bodyToggle.addEventListener('click', function () {
          const hidden = target.classList.toggle('is-hidden');
          bodyToggle.textContent = hidden ? 'Show diff' : 'Hide diff';
        });
      }
    });
  }

  function wireCommentGutters(panel, target) {
    const chatId = panel.getAttribute('data-chat-id');
    if (!chatId) return;
    target.querySelectorAll('.d2h-code-line, .d2h-code-side-line').forEach(function (row) {
      if (row.querySelector('.patch-line-comment-btn')) return;
      const lineNum = row.querySelector('.d2h-code-linenumber, .d2h-code-side-linenumber');
      const n = lineNum && (lineNum.getAttribute('data-line-number') || lineNum.textContent || '').trim();
      if (!n || !/^\d+$/.test(n)) return;
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'patch-line-comment-btn';
      btn.textContent = '+';
      btn.title = 'Comment on line ' + n;
      (lineNum || row).prepend(btn);
      btn.addEventListener('click', function (evt) {
        evt.preventDefault();
        evt.stopPropagation();
        openLineCommentBox(panel, row, n);
      });
    });
  }

  function openLineCommentBox(panel, row, line) {
    const chatId = panel.getAttribute('data-chat-id');
    let box = row.nextElementSibling;
    if (box && box.classList && box.classList.contains('patch-line-comment-box')) {
      box.querySelector('textarea').focus();
      return;
    }
    box = document.createElement('div');
    box.className = 'patch-line-comment-box';
    box.innerHTML = '<textarea placeholder="Comment…"></textarea><button type="button">Save</button>';
    row.insertAdjacentElement('afterend', box);
    const ta = box.querySelector('textarea');
    const save = box.querySelector('button');
    ta.focus();
    save.addEventListener('click', function () {
      const body = (ta.value || '').trim();
      if (!body) return;
      // Best-effort file path from nearest file header
      let file = '';
      let node = row;
      while (node) {
        const fileWrap = node.closest && node.closest('.d2h-file-wrapper');
        if (fileWrap) {
          const nameEl = fileWrap.querySelector('.d2h-file-name, .d2h-file-name-wrapper');
          file = (nameEl && nameEl.textContent || '').trim();
          break;
        }
        node = node.parentElement;
      }
      const fd = new FormData();
      fd.append('file', file || 'unknown');
      fd.append('side', 'new');
      fd.append('line', String(line));
      fd.append('body', body);
      fetch('/api/chats/' + encodeURIComponent(chatId) + '/patch/comment', {
        method: 'POST',
        body: fd,
        headers: { 'HX-Request': 'true' },
      }).then(function (r) { return r.text(); }).then(function (html) {
        const content = document.getElementById('chat-content');
        if (content && html) {
          content.outerHTML = html;
          initPatchReviews(document);
        }
      }).catch(function (e) { console.error(e); });
    });
  }

  document.addEventListener('DOMContentLoaded', function () { initPatchReviews(document); });
  document.body.addEventListener('htmx:afterSwap', function (evt) {
    initPatchReviews(evt.target || document);
  });

})();
