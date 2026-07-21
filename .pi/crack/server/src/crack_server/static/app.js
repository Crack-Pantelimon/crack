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
      if (key && openDetails[key]) d.open = true;
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
    initOtherToggle();
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
})();
