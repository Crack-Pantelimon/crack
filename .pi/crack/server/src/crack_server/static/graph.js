(function () {
  const query = document.getElementById('graph-query');
  const canvas = document.getElementById('graph-canvas');
  const status = document.getElementById('graph-status');
  const details = document.getElementById('graph-details');
  const nodeDetails = document.getElementById('graph-node-details');
  let cy;

  function show(data) {
    const elements = [
      ...data.nodes.map(n => ({ data: { id: n.id, label: n.label, summary: n.summary || '' } })),
      ...data.edges.filter(e => e.source && e.target).map(e => ({
        data: { id: e.id, source: e.source, target: e.target, label: e.label || 'related', fact: e.fact || '' }
      }))
    ];
    if (cy) cy.destroy();
    cy = cytoscape({
      container: canvas,
      elements,
      style: [
        { selector: 'node', style: { 'background-color': '#1976d2', label: 'data(label)', color: '#111', 'text-wrap': 'wrap', 'text-max-width': 120, 'font-size': 10, width: 28, height: 28 } },
        { selector: 'edge', style: { width: 1.5, 'line-color': '#888', 'target-arrow-color': '#888', 'target-arrow-shape': 'triangle', label: 'data(label)', 'font-size': 8, color: '#555', 'curve-style': 'bezier' } }
      ],
      layout: { name: 'cose', animate: false, padding: 30 }
    });
    cy.on('tap', 'node', async function (evt) {
      const n = evt.target;
      details.hidden = false;
      nodeDetails.textContent = JSON.stringify({ id: n.id(), label: n.data('label'), summary: n.data('summary') }, null, 2);
      status.textContent = 'Expanding ' + n.data('label') + '…';
      const r = await fetch('/graph/expand?uuid=' + encodeURIComponent(n.id()));
      if (r.ok) {
        const expanded = await r.json();
        show({ nodes: [...cy.nodes().map(x => ({ id: x.id(), label: x.data('label'), summary: x.data('summary') })), ...expanded.nodes], edges: [...cy.edges().map(x => x.data()), ...expanded.edges] });
        status.textContent = expanded.status && expanded.status.available ? 'Click nodes to expand.' : 'Graphiti is unavailable.';
      }
    });
    status.textContent = data.status && data.status.available ? (data.nodes.length + ' nodes · ' + data.edges.length + ' relationships') : 'Graphiti is unavailable; start the compose services.';
  }

  let timer;
  query.addEventListener('input', function () {
    clearTimeout(timer);
    timer = setTimeout(async function () {
      const q = query.value.trim();
      if (!q) { if (cy) cy.destroy(); canvas.replaceChildren(); status.textContent = 'Enter a query to begin.'; return; }
      status.textContent = 'Searching…';
      const r = await fetch('/graph/search?q=' + encodeURIComponent(q));
      if (r.ok) show(await r.json());
    }, 300);
  });
})();
read-error: ENOENT: ENOENT: no such file or directory