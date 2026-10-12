// The poster rows on Home can be dragged sideways with the mouse (a drag is not a click).
// Attached once; the listeners sit on the document, so rows that are drawn later work too.
window.cairnDragScroll = function () {
    if (window.__cairnDrag) return;
    window.__cairnDrag = true;
    let row = null, startX = 0, startLeft = 0, moved = false;

    document.addEventListener('mousedown', function (e) {
        moved = false;
        const r = e.target.closest ? e.target.closest('.shelf-row') : null;
        if (!r || e.button !== 0) return;
        row = r;
        startX = e.pageX;
        startLeft = r.scrollLeft;
    });

    document.addEventListener('mousemove', function (e) {
        if (!row) return;
        const dx = e.pageX - startX;
        if (Math.abs(dx) > 5) { moved = true; row.classList.add('dragging'); }
        if (moved) row.scrollLeft = startLeft - dx;
    });

    document.addEventListener('mouseup', function () {
        if (row) row.classList.remove('dragging');
        row = null;
    });

    // the click that ends a drag must not open the poster under the pointer
    document.addEventListener('click', function (e) {
        if (moved) { e.stopPropagation(); e.preventDefault(); moved = false; }
    }, true);
};

// Controller support: the D-pad or left stick moves the focus to the nearest control in that direction, A presses it,
// B closes a window (or goes Home), LB and RB switch between the top bar's pages.
window.cairnGamepad = function () {
    if (window.__cairnPad) return;
    window.__cairnPad = true;
    let nextAt = 0;
    const down = {};

    const visible = (el) => {
        const r = el.getBoundingClientRect();
        if (r.width < 4 || r.height < 4) return false;
        const s = getComputedStyle(el);
        return s.visibility !== 'hidden' && s.display !== 'none' && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth;
    };
    const overlay = () => document.querySelector('.modal-overlay, .qs-backdrop, .ctx-backdrop, .tour-overlay');
    const candidates = () => {
        const scope = overlay() || document;
        return [...scope.querySelectorAll('button:not(:disabled), a[href], input:not([type=hidden]), select, [tabindex="0"]')].filter(visible);
    };

    function move(dx, dy) {
        document.body.classList.add('pad-focus');
        const list = candidates();
        if (list.length === 0) return;
        const cur = document.activeElement && list.includes(document.activeElement) ? document.activeElement : null;
        if (!cur) { list[0].focus(); return; }
        const a = cur.getBoundingClientRect();
        const ax = a.left + a.width / 2, ay = a.top + a.height / 2;
        let best = null, bestScore = Infinity;
        for (const el of list) {
            if (el === cur) continue;
            const r = el.getBoundingClientRect();
            const ex = r.left + r.width / 2 - ax, ey = r.top + r.height / 2 - ay;
            const along = dx !== 0 ? ex * dx : ey * dy;
            const across = dx !== 0 ? Math.abs(ey) : Math.abs(ex);
            if (along <= 4) continue;                       // not in that direction
            const score = along + across * 2.2;
            if (score < bestScore) { bestScore = score; best = el; }
        }
        if (best) { best.focus(); best.scrollIntoView({ block: 'nearest', inline: 'nearest' }); }
    }

    const pressed = (p, i) => !!(p.buttons[i] && p.buttons[i].pressed);
    const edge = (p, i) => { const n = pressed(p, i), was = !!down[i]; down[i] = n; return n && !was; };

    function tick(now) {
        const pad = [...(navigator.getGamepads ? navigator.getGamepads() : [])].find(x => x && x.connected);
        if (pad) {
            const x = pad.axes[0] || 0, y = pad.axes[1] || 0;
            let dx = pressed(pad, 15) ? 1 : pressed(pad, 14) ? -1 : (x > 0.6 ? 1 : x < -0.6 ? -1 : 0);
            let dy = pressed(pad, 13) ? 1 : pressed(pad, 12) ? -1 : (y > 0.6 ? 1 : y < -0.6 ? -1 : 0);
            if ((dx || dy) && now >= nextAt) { move(dx, dy); nextAt = now + 220; }
            if (edge(pad, 0) && document.activeElement && document.activeElement !== document.body) document.activeElement.click();
            if (edge(pad, 1)) {
                if (overlay()) window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
                else { const h = document.querySelector('.nav .nav-btn'); if (h) h.click(); }
            }
            const pages = [...document.querySelectorAll('.nav:not(.right) .nav-btn')];
            if (edge(pad, 4) && pages[0]) pages[0].click();
            if (edge(pad, 5) && pages[1]) pages[1].click();
        }
        requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
};
